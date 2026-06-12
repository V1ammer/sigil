//! Круглая зона выбора аватара: клик открывает системный file picker,
//! drag&drop картинки работает так же. Выбранное изображение кропится по
//! центру в квадрат и сжимается до 256×256 JPEG через canvas — результат
//! отдаётся как data URL (он же превью, он же формат локального хранения).

use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::{JsCast, JsValue};

/// Сторона результирующего квадрата в пикселях.
const SIDE: f64 = 256.0;

/// Декодирует image-файл браузером и возвращает data URL JPEG 256×256.
async fn process_image(file: web_sys::File) -> Result<String, JsValue> {
    let url = web_sys::Url::create_object_url_with_blob(&file)?;
    let img = web_sys::HtmlImageElement::new()?;
    img.set_src(&url);
    let decode_result = wasm_bindgen_futures::JsFuture::from(img.decode()).await;
    let _ = web_sys::Url::revoke_object_url(&url);
    decode_result?;

    let (w, h) = (f64::from(img.natural_width()), f64::from(img.natural_height()));
    if w < 1.0 || h < 1.0 {
        return Err(JsValue::from_str("empty image"));
    }
    let side = w.min(h);
    let (sx, sy) = ((w - side) / 2.0, (h - side) / 2.0);

    let doc = web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| JsValue::from_str("no document"))?;
    let canvas: web_sys::HtmlCanvasElement = doc
        .create_element("canvas")?
        .unchecked_into();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        canvas.set_width(SIDE as u32);
        canvas.set_height(SIDE as u32);
    }
    let ctx: web_sys::CanvasRenderingContext2d = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("no 2d context"))?
        .unchecked_into();
    ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
        &img, sx, sy, side, side, 0.0, 0.0, SIDE, SIDE,
    )?;
    canvas.to_data_url_with_type_and_encoder_options("image/jpeg", &JsValue::from_f64(0.85))
}

fn first_image_file(files: Option<web_sys::FileList>) -> Option<web_sys::File> {
    let files = files?;
    for i in 0..files.length() {
        if let Some(f) = files.get(i) {
            if f.type_().starts_with("image/") {
                return Some(f);
            }
        }
    }
    None
}

/// Круглый пикер аватара. `value` — data URL текущей картинки (двусторонний:
/// родитель может проинициализировать сохранённым аватаром).
#[must_use]
#[component]
pub fn AvatarPicker(
    /// data URL выбранной картинки; `None` — пустой кружок с иконкой.
    value: RwSignal<Option<String>>,
    /// Tailwind-классы размера, например "h-24 w-24".
    #[prop(optional, into)] size_class: String,
) -> impl IntoView {
    let input_ref: NodeRef<leptos::html::Input> = NodeRef::new();
    let drag_over = RwSignal::new(false);
    let size_class = if size_class.is_empty() { "h-24 w-24".to_string() } else { size_class };

    let handle_file = move |file: web_sys::File| {
        spawn_local(async move {
            match process_image(file).await {
                Ok(data_url) => value.set(Some(data_url)),
                Err(e) => web_sys::console::error_1(&e),
            }
        });
    };

    let on_change = move |ev: leptos::ev::Event| {
        let Some(target) = ev.target() else { return };
        let input: web_sys::HtmlInputElement = target.unchecked_into();
        let file = first_image_file(input.files());
        // Сброс, чтобы повторный выбор того же файла снова сработал.
        input.set_value("");
        if let Some(f) = file {
            handle_file(f);
        }
    };

    let on_click = move |_| {
        if let Some(el) = input_ref.get() {
            let inp: &web_sys::HtmlInputElement = el.unchecked_ref();
            inp.click();
        }
    };

    let on_drop = move |ev: leptos::ev::DragEvent| {
        ev.prevent_default();
        drag_over.set(false);
        if let Some(f) = first_image_file(ev.data_transfer().and_then(|dt| dt.files())) {
            handle_file(f);
        }
    };

    view! {
        <div
            class=move || {
                let ring = if drag_over.get() { "ring-2 ring-primary" } else { "" };
                format!(
                    "relative flex {size_class} cursor-pointer items-center justify-center \
                     overflow-hidden rounded-full bg-muted transition-shadow {ring}"
                )
            }
            on:click=on_click
            on:dragover=move |ev: leptos::ev::DragEvent| {
                ev.prevent_default();
                drag_over.set(true);
            }
            on:dragleave=move |_| drag_over.set(false)
            on:drop=on_drop
        >
            {move || match value.get() {
                Some(data_url) => view! {
                    <img class="h-full w-full object-cover" src=data_url alt="avatar"/>
                }.into_any(),
                None => view! {
                    // Иконка камеры — намёк «нажми, чтобы выбрать фото».
                    <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground"><path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z"/><circle cx="12" cy="13" r="3"/></svg>
                }.into_any(),
            }}
            <input
                node_ref=input_ref
                type="file"
                accept="image/*"
                class="hidden"
                on:change=on_change
            />
        </div>
    }
}
