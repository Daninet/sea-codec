import { decodeAudioFile } from "./dist/deps.modern.js";
import {
  encodeWAV,
  getPSNR,
  channelSamplesToInterleavedInt16,
  calculateDifference,
} from "./utils.mjs";

let wasm;

(async () => {
  const wasmModule = await WebAssembly.instantiateStreaming(fetch("codec.wasm"), {
    env: {
      memory: new WebAssembly.Memory({ initial: 256 }), // 16 MB
      js_error: (start) => {
        const view = new Uint8Array(wasmModule.instance.exports.memory.buffer);
        let end = view.indexOf(0, start);
        if (end === -1) throw new Error("Got invalid string from wasm");
        const str = new TextDecoder().decode(view.subarray(start, end));
        throw new Error(str);
      },
    },
  });

  const wasmExports = wasmModule.instance.exports;
  wasmExports.setup();

  wasm = {
    wasm_sea_encode: (inputSamples, sampleRate, channels, quality, vbr) => {
      if (!(inputSamples instanceof Int16Array))
        throw new Error("inputSamples should be Int16Array");

      let wasmInputBufferSize;
      let wasmInputBuffer;
      let wasmOutputBufferSize;
      let wasmOutputBuffer;

      try {
        wasmInputBufferSize = inputSamples.byteLength;
        wasmInputBuffer = wasmExports.allocate(wasmInputBufferSize);
        const inputSamplesU8 = new Uint8Array(
          inputSamples.buffer,
          inputSamples.byteOffset,
          inputSamples.byteLength
        ).slice();

        new Uint8Array(wasmExports.memory.buffer).set(inputSamplesU8, wasmInputBuffer);

        wasmOutputBufferSize = Math.floor(inputSamples.byteLength);
        wasmOutputBuffer = wasmExports.allocate(wasmOutputBufferSize);

        const outputLength = wasmExports.wasm_sea_encode(
          wasmInputBuffer,
          wasmInputBufferSize,
          sampleRate,
          channels,
          quality,
          vbr,
          wasmOutputBuffer,
          wasmOutputBufferSize
        );

        if (outputLength === 0) throw new Error("Encoding failed: Output buffer too small.");

        const output = new Uint8Array(
          wasmExports.memory.buffer,
          wasmOutputBuffer,
          outputLength
        ).slice();

        return output;
      } catch (e) {
        postMessage(["error", e.message]);
        throw e;
      } finally {
        wasmExports.deallocate(wasmInputBuffer, wasmInputBufferSize);
        wasmExports.deallocate(wasmOutputBuffer, wasmOutputBufferSize);
      }
    },
    wasm_sea_decode: (encodedData) => {
      if (!(encodedData instanceof Uint8Array)) throw new Error("encodedData should be Uint8Array");

      let wasmInputBufferSize;
      let wasmInputBuffer;
      let wasmOutputBufferSize;
      let wasmOutputBuffer;
      let wasmSampleRateBuffer;
      let wasmChannelsBuffer;

      try {
        wasmInputBufferSize = encodedData.byteLength;
        wasmInputBuffer = wasmExports.allocate(wasmInputBufferSize);
        new Uint8Array(wasmExports.memory.buffer).set(encodedData, wasmInputBuffer);

        wasmOutputBufferSize = wasmInputBufferSize * 15;
        wasmOutputBuffer = wasmExports.allocate(wasmOutputBufferSize);

        wasmSampleRateBuffer = wasmExports.allocate(4);
        wasmChannelsBuffer = wasmExports.allocate(4);

        const outputLength = wasmExports.wasm_sea_decode(
          wasmInputBuffer,
          wasmInputBufferSize,
          wasmOutputBuffer,
          wasmOutputBufferSize,
          wasmSampleRateBuffer,
          wasmChannelsBuffer
        );

        if (outputLength === 0) throw new Error("Decoding failed: Got zero output length.");

        const output = new Int16Array(
          wasmExports.memory.buffer,
          wasmOutputBuffer,
          outputLength / 2
        ).slice();

        const sampleRate = new Uint32Array(wasmExports.memory.buffer, wasmSampleRateBuffer, 1)[0];
        const channels = new Uint32Array(wasmExports.memory.buffer, wasmChannelsBuffer, 1)[0];

        if (sampleRate === 0 || channels === 0)
          throw new Error("Decoding failed: Got invalid output.");

        return {
          samples: output,
          sampleRate,
          channels,
        };
      } catch (e) {
        postMessage(["error", e.message]);
        throw e;
      } finally {
        wasmExports.deallocate(wasmInputBuffer, wasmInputBufferSize);
        wasmExports.deallocate(wasmOutputBuffer, wasmOutputBufferSize);
        wasmExports.deallocate(wasmSampleRateBuffer, 4);
        wasmExports.deallocate(wasmChannelsBuffer, 4);
      }
    },
    wasm_resample: (inputSamples, sourceRate, targetRate, channels) => {
      if (!(inputSamples instanceof Int16Array))
        throw new Error("inputSamples should be Int16Array");

      let wasmInputBufferSize;
      let wasmInputBuffer;
      let wasmOutputBufferSize;
      let wasmOutputBuffer;

      try {
        wasmInputBufferSize = inputSamples.byteLength;
        wasmInputBuffer = wasmExports.allocate(wasmInputBufferSize);
        const inputSamplesU8 = new Uint8Array(
          inputSamples.buffer,
          inputSamples.byteOffset,
          inputSamples.byteLength
        ).slice();

        new Uint8Array(wasmExports.memory.buffer).set(inputSamplesU8, wasmInputBuffer);

        // Estimate output size: slightly more than needed ratio to be safe
        wasmOutputBufferSize = Math.ceil((inputSamples.byteLength * targetRate) / sourceRate) + 1024;
        wasmOutputBuffer = wasmExports.allocate(wasmOutputBufferSize);

        const outputLength = wasmExports.wasm_resample(
          wasmInputBuffer,
          wasmInputBufferSize,
          sourceRate,
          targetRate,
          channels,
          wasmOutputBuffer,
          wasmOutputBufferSize
        );

        if (outputLength === 0) throw new Error("Resampling failed: Output buffer too small.");

        const output = new Int16Array(
          wasmExports.memory.buffer,
          wasmOutputBuffer,
          outputLength / 2
        ).slice();

        return output;
      } catch (e) {
        postMessage(["error", e.message]);
        throw e;
      } finally {
        wasmExports.deallocate(wasmInputBuffer, wasmInputBufferSize);
        wasmExports.deallocate(wasmOutputBuffer, wasmOutputBufferSize);
      }
    },
  };

  // warm up JIT
  const encodedData = wasm.wasm_sea_encode(new Int16Array(1024 * 1024), 44100, 1, 3, false);
  wasm.wasm_sea_decode(encodedData);
})();

let exports = {
  async decodeAudioFile(arrayBuffer) {
    const audioBuffer = await decodeAudioFile(arrayBuffer);
    const samplesPerChannel = [...Array(audioBuffer.numberOfChannels).keys()].map((i) =>
      audioBuffer.getChannelData(i)
    );

    const interleavedSamples = channelSamplesToInterleavedInt16(
      samplesPerChannel,
      audioBuffer.numberOfChannels
    );

    return {
      samples: interleavedSamples,
      sampleRate: audioBuffer.sampleRate,
      channels: audioBuffer.numberOfChannels,
    };
  },

  resampleAudio(interleavedSamples, sourceRate, targetRate, channels) {
    const start = performance.now();
    const resampled = wasm.wasm_resample(interleavedSamples, sourceRate, targetRate, channels);
    const end = performance.now();

    return {
      samples: resampled,
      duration: end - start,
    };
  },

  async encodeSEA(interleavedSamples, sampleRate, channels, quality, vbr) {
    const start = performance.now();
    const encoded = wasm.wasm_sea_encode(interleavedSamples, sampleRate, channels, quality, vbr);
    const end = performance.now();

    return {
      encoded,
      duration: end - start,
    };
  },

  decodeSEA(encodedData) {
    const start = performance.now();
    const { samples, sampleRate, channels } = wasm.wasm_sea_decode(encodedData);
    const end = performance.now();
    const wave = encodeWAV(samples, sampleRate, channels);

    return {
      samples,
      wave,
      sampleRate,
      channels,
      duration: end - start,
    };
  },

  compareAudio(originalSamples, originalSampleRate, decodedSamples, decodedSampleRate, channels) {
    let processedDecoded = decodedSamples;
    if (decodedSampleRate !== originalSampleRate) {
      processedDecoded = wasm.wasm_resample(
        decodedSamples,
        decodedSampleRate,
        originalSampleRate,
        channels
      );
    }

    // Ensure lengths match for comparison
    const minLength = Math.min(originalSamples.length, processedDecoded.length);
    const a = originalSamples.slice(0, minLength);
    const b = processedDecoded.slice(0, minLength);

    const psnr = getPSNR(a, b);
    const diff = calculateDifference(a, b);
    const differenceFromOriginal = encodeWAV(diff, originalSampleRate, channels);

    return {
      psnr,
      differenceFromOriginal,
    };
  },
};

addEventListener("message", async (e) => {
  const { data } = e;
  const [id, fn, ...rest] = data;

  if (fn in exports) {
    try {
      const res = await exports[fn](...rest);
      postMessage([id, res]);
    } catch (e) {
      postMessage(["error", e.message]);
      console.error(e);
    }
  } else {
    throw new Error(`Unknown function: ${fn}`);
  }
});
