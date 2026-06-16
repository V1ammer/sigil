/* Hardware video transcode via WebCodecs, off the main thread.
 *
 * Demux (mp4box) → VideoDecoder (any codec the browser decodes in hardware,
 * incl. HEVC) → downscale on an OffscreenCanvas → VideoEncoder (H.264) → mux a
 * FRAGMENTED mp4 (mp4-muxer). Audio AAC is copied through untouched. The result
 * streams via the existing MediaSource driver. If the input's video codec can't
 * be decoded here, we throw `unsupported` so the caller falls back to ffmpeg.wasm.
 */
importScripts("mp4box.all.min.js", "mp4-muxer.js");

const usToTrackTime = 1e6;

function trackDescription(mp4, trackId) {
  const trak = mp4.getTrackById(trackId);
  for (const entry of trak.mdia.minf.stbl.stsd.entries) {
    const box = entry.avcC || entry.hvcC || entry.vpcC || entry.av1C;
    if (box) {
      const ds = new DataStream(undefined, 0, DataStream.BIG_ENDIAN);
      box.write(ds);
      return new Uint8Array(ds.buffer, 8); // strip the 8-byte box header
    }
  }
  return undefined;
}

// Pull the AAC AudioSpecificConfig out of the esds so the muxer can write esds.
function audioSpecificConfig(mp4, trackId) {
  try {
    const trak = mp4.getTrackById(trackId);
    const esds = trak.mdia.minf.stbl.stsd.entries[0].esds;
    const asc = esds.esd.descs[0].descs[0].data;
    return new Uint8Array(asc);
  } catch (_) {
    return undefined;
  }
}

// Build the precise `codecs=` string from an emitted avcC (profile/constraint/level).
function avcCodecString(desc) {
  if (desc && desc.length >= 4) {
    const hex = (n) => n.toString(16).padStart(2, "0");
    return `avc1.${hex(desc[1])}${hex(desc[2])}${hex(desc[3])}`;
  }
  return "avc1.4d0028";
}

async function demux(buffer) {
  return new Promise((resolve, reject) => {
    const mp4 = MP4Box.createFile();
    const videoSamples = [];
    const audioSamples = [];
    let info = null;
    mp4.onError = (e) => reject(new Error("demux: " + e));
    mp4.onReady = (i) => {
      info = i;
      const vt = i.videoTracks && i.videoTracks[0];
      if (!vt) {
        reject(new Error("no video track"));
        return;
      }
      mp4.setExtractionOptions(vt.id, "v", { nbSamples: Infinity });
      const at = i.audioTracks && i.audioTracks[0];
      if (at) mp4.setExtractionOptions(at.id, "a", { nbSamples: Infinity });
      mp4.start();
    };
    mp4.onSamples = (_id, user, samples) => {
      const dst = user === "v" ? videoSamples : audioSamples;
      for (const s of samples) dst.push(s);
    };
    const ab = buffer;
    ab.fileStart = 0;
    mp4.appendBuffer(ab);
    mp4.flush();
    // onReady/onSamples fire synchronously within appendBuffer/flush for an
    // in-memory file, so everything is collected by here.
    if (!info) {
      reject(new Error("demux: not ready"));
      return;
    }
    resolve({ mp4, info, videoSamples, audioSamples });
  });
}

async function transcode(buffer, maxLong, bitrateK, onProgress) {
  const { mp4, info, videoSamples, audioSamples } = await demux(buffer);
  const vt = info.videoTracks[0];
  const at = info.audioTracks[0];
  const vDesc = trackDescription(mp4, vt.id);

  const decConfig = {
    codec: vt.codec,
    codedWidth: vt.video.width,
    codedHeight: vt.video.height,
    description: vDesc,
    // No hardwareAcceleration hint: let the browser use HW when available and
    // fall back to its own software path, rather than reporting "unsupported"
    // on machines without a hardware decoder for this codec.
  };
  const support = await VideoDecoder.isConfigSupported(decConfig).catch(() => ({ supported: false }));
  if (!support.supported) throw new Error("unsupported: " + vt.codec);

  // Output dimensions: scale long side to the cap, keep aspect, force even.
  const iw = vt.video.width, ih = vt.video.height;
  const scale = Math.min(1, maxLong / Math.max(iw, ih));
  const ow = Math.max(2, Math.round((iw * scale) / 2) * 2);
  const oh = Math.max(2, Math.round((ih * scale) / 2) * 2);

  const durSec = vt.duration / vt.timescale;
  const fps = durSec > 0 ? Math.min(60, Math.round(vt.nb_samples / durSec)) : 30;
  const gop = Math.max(1, Math.round(fps * 2)); // keyframe every ~2s → fragment boundary

  const canvas = new OffscreenCanvas(ow, oh);
  const ctx = canvas.getContext("2d", { alpha: false });

  const target = new Mp4Muxer.ArrayBufferTarget();
  const muxer = new Mp4Muxer.Muxer({
    target,
    video: { codec: "avc", width: ow, height: oh },
    audio: at ? { codec: "aac", sampleRate: at.audio.sample_rate, numberOfChannels: at.audio.channel_count } : undefined,
    fastStart: "fragmented",
    firstTimestampBehavior: "offset",
  });

  const videoChunks = [];
  let firstMeta = null;

  const encoder = new VideoEncoder({
    output: (chunk, meta) => {
      if (!firstMeta && meta) firstMeta = meta;
      videoChunks.push(chunk);
    },
    error: (e) => { throw e; },
  });
  encoder.configure({
    codec: "avc1.4d0028", // H.264 Main@4.0
    width: ow,
    height: oh,
    bitrate: bitrateK * 1000,
    framerate: fps,
    avc: { format: "avc" },
  });

  let decoded = 0;
  const total = videoSamples.length || 1;
  const decoder = new VideoDecoder({
    output: (frame) => {
      ctx.drawImage(frame, 0, 0, ow, oh);
      const scaled = new VideoFrame(canvas, { timestamp: frame.timestamp, duration: frame.duration || 0 });
      frame.close();
      encoder.encode(scaled, { keyFrame: decoded % gop === 0 });
      scaled.close();
      decoded++;
      if (decoded % 15 === 0 && onProgress) onProgress((decoded / total) * 0.9);
    },
    error: (e) => { throw e; },
  });
  decoder.configure(decConfig);

  for (const s of videoSamples) {
    decoder.decode(
      new EncodedVideoChunk({
        type: s.is_sync ? "key" : "delta",
        timestamp: (s.cts * usToTrackTime) / s.timescale,
        duration: (s.duration * usToTrackTime) / s.timescale,
        data: s.data,
      })
    );
    // Backpressure: don't let the decode queue run away on long 4K clips.
    if (decoder.decodeQueueSize > 20) {
      await new Promise((r) => setTimeout(r, 0));
    }
  }
  await decoder.flush();
  await encoder.flush();
  decoder.close();
  encoder.close();

  // Mux: add video + audio chunks merged by timestamp (fragmented muxer wants
  // monotonic, interleaved adds).
  const vMeta = firstMeta;
  const aDesc = at ? audioSpecificConfig(mp4, at.id) : undefined;
  const audioChunks = at
    ? audioSamples.map(
        (s) =>
          new EncodedAudioChunk({
            type: "key",
            timestamp: (s.cts * usToTrackTime) / s.timescale,
            duration: (s.duration * usToTrackTime) / s.timescale,
            data: s.data,
          })
      )
    : [];

  let vi = 0, ai = 0;
  let aMetaSent = false;
  while (vi < videoChunks.length || ai < audioChunks.length) {
    const vNext = vi < videoChunks.length ? videoChunks[vi].timestamp : Infinity;
    const aNext = ai < audioChunks.length ? audioChunks[ai].timestamp : Infinity;
    if (vNext <= aNext) {
      muxer.addVideoChunk(videoChunks[vi], vi === 0 ? vMeta : undefined);
      vi++;
    } else {
      const meta = !aMetaSent && aDesc ? { decoderConfig: { description: aDesc } } : undefined;
      aMetaSent = true;
      muxer.addAudioChunk(audioChunks[ai], meta);
      ai++;
    }
  }
  muxer.finalize();
  if (onProgress) onProgress(1);

  const desc = vMeta && vMeta.decoderConfig && vMeta.decoderConfig.description
    ? new Uint8Array(vMeta.decoderConfig.description)
    : undefined;
  const mime = `video/mp4; codecs="${avcCodecString(desc)}, mp4a.40.2"`;
  return { buffer: target.buffer, mime };
}

self.onmessage = async (e) => {
  const { buffer, maxLong, bitrateK } = e.data;
  try {
    const { buffer: out, mime } = await transcode(buffer, maxLong || 1280, bitrateK || 2500, (v) =>
      self.postMessage({ type: "progress", value: v })
    );
    self.postMessage({ type: "done", buffer: out, mime }, [out]);
  } catch (err) {
    self.postMessage({ type: "error", message: String((err && err.message) || err) });
  }
};
