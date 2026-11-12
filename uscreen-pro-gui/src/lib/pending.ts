/**
 * Which row shows the "needs Apply" dot for each setting that waits for Apply: the key in config.toml and the label of its row.
 * Keep it in step with HEAVY in App.tsx.
 */
export const APPLY_LABELS: Record<string, string[]> = {
  encoder: ["Encoder"],
  fps: ["Frame rate"],
  bitrate: ["Bitrate ceiling"],
  quality: ["Quality"],
  stream_scale: ["Stream detail"],
  ten_bit: ["10-bit colour depth"],
  position: ["Position"],
  auto_resolution: ["Auto resolution"],
  width: ["Fixed resolution"],
  height: ["Fixed resolution"],
  keep_alive: ["Keep-alive display"],
  software_cursor: ["Use Software Mouse Cursor (Slower, more compatible)"],
  pointer_handoff: ["Seamless pointer handoff"],
  max_performance_mode: ["Max performance scheduling"],
  input_mode: ["Default mode", "Mode"],
};
