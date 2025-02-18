export function readFile(file) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result);
    reader.onerror = () => reject(reader.error);
    reader.readAsArrayBuffer(file);
  });
}

export function downloadFile(data, filename, mimeType) {
  const blob = new Blob([data], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

export function formatNumber(x) {
  return x.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

export function encodeWAV(samples, sampleRate, channels) {
  const buffer = new ArrayBuffer(44 + samples.length * 2);
  const view = new DataView(buffer);

  const writeString = (offset, string) => {
    for (let i = 0; i < string.length; i++) {
      view.setUint8(offset + i, string.charCodeAt(i));
    }
  };

  writeString(0, "RIFF");
  view.setUint32(4, 36 + samples.length * 2, true);
  writeString(8, "WAVE");
  writeString(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, channels, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * channels * 2, true);
  view.setUint16(32, channels * 2, true);
  view.setUint16(34, 16, true);
  writeString(36, "data");
  view.setUint32(40, samples.length * 2, true);

  let sampleBuffer = new Int16Array(buffer, 44, samples.length);
  sampleBuffer.set(samples);

  return new Uint8Array(buffer);
}

export function channelSamplesToInterleavedInt16(channelSamples, channels) {
  const interleavedSamples = new Int16Array(channelSamples[0].length * channels);

  for (let channel = 0; channel < channels; channel++) {
    for (let i = 0; i < channelSamples[0].length; i++) {
      let clamped = Math.max(-1, Math.min(1, channelSamples[channel][i]));
      let i16 = Math.floor(clamped < 0 ? clamped * 0x8000 : clamped * 0x7fff);
      interleavedSamples[i * channels + channel] = i16;
    }
  }

  return interleavedSamples;
}

export function getPSNR(a, b) {
  if (a.length !== b.length) {
    throw new Error("Size mismatch");
  }
  let sum = 0;
  for (let i = 0; i < a.length; i++) {
    const diff = a[i] / 32768 - b[i] / 32768;
    sum += diff * diff;
  }

  let rms = Math.sqrt(sum / a.length);
  let psnr = -20 * Math.log10(2.0 / rms);
  return psnr;
}

export function calculateDifference(originalData, decodedData) {
  const diff = new Int16Array(originalData.length);
  for (let i = 0; i < originalData.length; i++) {
    diff[i] = originalData[i] - decodedData[i];
  }
  return diff;
}
