//! Storage backend abstraction для вложений.
//!
//! Поддерживает два режима:
//! - `InDatabase` — для маленьких файлов (< threshold), хранит данные в БД.
//! - `FileSystem` — для больших файлов, хранит на диске с двухуровневой разбивкой.

use std::path::{Path, PathBuf};

use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use uuid::Uuid;

use crate::error::AppError;

/// Ссылка на сохранённые данные.
#[derive(Debug, Clone)]
pub enum StoredRef {
    /// Данные хранятся в БД (в `payload_ciphertext` колонке).
    Inline(Vec<u8>),
    /// Данные хранятся на файловой системе.
    OnDisk {
        /// Относительный путь от `data_dir`, например `att/aa/bb/<uuid>.bin`.
        relative_path: PathBuf,
        /// Размер файла в байтах.
        size: u64,
    },
}

impl StoredRef {
    /// Размер данных в байтах.
    #[must_use]
    pub fn size(&self) -> u64 {
        match self {
            Self::Inline(data) => data.len() as u64,
            Self::OnDisk { size, .. } => *size,
        }
    }
}

/// Storage backend для вложений.
#[derive(Debug, Clone)]
pub enum StorageBackend {
    /// Все данные хранятся в БД (в колонке `payload_ciphertext`).
    InDatabase,
    /// Данные хранятся на файловой системе; маленькие файлы — в БД.
    FileSystem {
        /// Корневая директория (`data_dir`).
        root: PathBuf,
        /// Порог для inline-хранения (байты). Файлы меньше этого — в БД.
        inline_threshold: u64,
    },
}

impl StorageBackend {
    /// Сохраняет данные и возвращает `StoredRef`.
    ///
    /// # Errors
    ///
    /// - `AppError::Internal` — ошибка ввода-вывода или превышение размера.
    pub async fn store(&self, id: Uuid, data: &[u8]) -> Result<StoredRef, AppError> {
        match self {
            Self::InDatabase => Ok(StoredRef::Inline(data.to_vec())),
            Self::FileSystem { root, inline_threshold } => {
                let len = data.len() as u64;
                if len < *inline_threshold {
                    return Ok(StoredRef::Inline(data.to_vec()));
                }
                let relative_path = Self::disk_path(id);
                let full_path = root.join(&relative_path);
                if let Some(parent) = full_path.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(|e| AppError::Internal(anyhow::anyhow!(
                            "cannot create attachment dir {}: {e}",
                            parent.display()
                        )))?;
                }
                let mut file = tokio::fs::File::create(&full_path)
                    .await
                    .map_err(|e| AppError::Internal(anyhow::anyhow!(
                        "cannot create attachment file {}: {e}",
                        full_path.display()
                    )))?;
                file.write_all(data)
                    .await
                    .map_err(|e| AppError::Internal(anyhow::anyhow!(
                        "cannot write attachment {}: {e}",
                        full_path.display()
                    )))?;
                Ok(StoredRef::OnDisk { relative_path, size: len })
            }
        }
    }

    /// Читает весь blob.
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` — файл на диске не найден.
    /// - `AppError::Internal` — ошибка ввода-вывода.
    pub async fn read(&self, sref: &StoredRef) -> Result<Vec<u8>, AppError> {
        match sref {
            StoredRef::Inline(data) => Ok(data.clone()),
            StoredRef::OnDisk { relative_path, .. } => {
                let full_path = self.resolve_path(relative_path)?;
                tokio::fs::read(&full_path)
                    .await
                    .map_err(|e| {
                        if e.kind() == std::io::ErrorKind::NotFound {
                            AppError::NotFound
                        } else {
                            AppError::Internal(anyhow::anyhow!(
                                "cannot read attachment {}: {e}",
                                full_path.display()
                            ))
                        }
                    })
            }
        }
    }

    /// Читает диапазон байт `[start, end]` включительно.
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` — файл на диске не найден.
    /// - `AppError::BadRequest` — некорректный диапазон.
    /// - `AppError::Internal` — ошибка ввода-вывода.
    #[allow(clippy::cast_possible_truncation)]
    pub async fn read_range(
        &self,
        sref: &StoredRef,
        start: u64,
        end: u64,
    ) -> Result<Vec<u8>, AppError> {
        let size = sref.size();
        if start >= size || end >= size || start > end {
            return Err(AppError::BadRequest(format!(
                "invalid range {start}-{end} for size {size}"
            )));
        }
        let len = end - start + 1;
        match sref {
            StoredRef::Inline(data) => {
                let start_us = start as usize;
                let end_us = (end as usize) + 1;
                if end_us > data.len() {
                    return Err(AppError::BadRequest("range exceeds data length".into()));
                }
                Ok(data[start_us..end_us].to_vec())
            }
            StoredRef::OnDisk { relative_path, .. } => {
                let full_path = self.resolve_path(relative_path)?;
                let file = tokio::fs::File::open(&full_path)
                    .await
                    .map_err(|e| {
                        if e.kind() == std::io::ErrorKind::NotFound {
                            AppError::NotFound
                        } else {
                            AppError::Internal(anyhow::anyhow!(
                                "cannot open attachment {}: {e}",
                                full_path.display()
                            ))
                        }
                    })?;
                let mut buf = vec![0u8; len as usize];
                let mut reader = tokio::io::BufReader::new(file);
                reader.seek(std::io::SeekFrom::Start(start))
                    .await
                    .map_err(|e| AppError::Internal(anyhow::anyhow!(
                        "cannot seek attachment: {e}"
                    )))?;
                reader.read_exact(&mut buf)
                    .await
                    .map_err(|e| AppError::Internal(anyhow::anyhow!(
                        "cannot read attachment range: {e}"
                    )))?;
                Ok(buf)
            }
        }
    }

    /// Удаляет данные.
    ///
    /// # Errors
    ///
    /// - `AppError::Internal` — ошибка ввода-вывода.
    pub async fn delete(&self, sref: &StoredRef) -> Result<(), AppError> {
        if let StoredRef::OnDisk { relative_path, .. } = sref {
            let full_path = self.resolve_path(relative_path)?;
            let _ = tokio::fs::remove_file(&full_path).await;
        }
        Ok(())
    }

    /// Возвращает полный путь к файлу на диске.
    fn resolve_path(&self, relative: &Path) -> Result<PathBuf, AppError> {
        match self {
            Self::InDatabase => Err(AppError::Internal(anyhow::anyhow!(
                "cannot resolve disk path for InDatabase backend"
            ))),
            Self::FileSystem { root, .. } => Ok(root.join(relative)),
        }
    }

    /// Формирует относительный путь: `att/<first2hex>/<next2hex>/<uuid>.bin`.
    #[must_use]
    pub fn disk_path(id: Uuid) -> PathBuf {
        let hex = id.to_string().replace('-', "");
        let first = &hex[0..2];
        let second = &hex[2..4];
        PathBuf::from("att")
            .join(first)
            .join(second)
            .join(format!("{id}.bin"))
    }
}
