import npmAudioDecodeLib from "audio-decode";

export function decodeAudioFile(buffer) {
  return npmAudioDecodeLib(buffer);
}
