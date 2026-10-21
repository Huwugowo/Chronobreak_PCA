// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createVerticalPreviewGpu } from "./verticalPreviewGpu";
afterEach(() => vi.restoreAllMocks());
function fixture() {
  const canvas = document.createElement("canvas");
  let constant = 0;
  const methods: Record<string, ReturnType<typeof vi.fn>> = {};
  const constants: Record<string, number> = {};
  const gl = new Proxy({}, { get(_, key: string) {
    if (key.toUpperCase() === key) return constants[key] ??= ++constant;
    return methods[key] ??= vi.fn(() => key.startsWith("create") || key === "getUniformLocation" ? {} :
      key === "getShaderParameter" || key === "getProgramParameter" ? true :
      key === "getAttribLocation" ? 0 : key === "checkFramebufferStatus" ? constants.FRAMEBUFFER_COMPLETE :
      key === "getError" ? constants.NO_ERROR : undefined);
  } }) as WebGLRenderingContext;
  // Resolve constants compared to return values before initialization.
  void gl.FRAMEBUFFER_COMPLETE; void gl.NO_ERROR;
  vi.spyOn(canvas, "getContext").mockReturnValue(gl);
  const video = document.createElement("video");
  Object.defineProperties(video, { videoWidth: {value:1920}, videoHeight: {value:1080} });
  return { canvas, gl, methods, video };
}
it("uploads once per frame, renders two bounded passes and releases each resource once", () => {
  const f = fixture(), gpu = createVerticalPreviewGpu(f.canvas);
  expect(f.methods.pixelStorei).toHaveBeenCalledWith(f.gl.UNPACK_FLIP_Y_WEBGL, false);
  gpu.draw(f.video, 276, 450);
  expect([f.canvas.width, f.canvas.height]).toEqual([640, 360]);
  expect(f.methods.texImage2D).toHaveBeenCalledTimes(2); // fixed intermediate + source allocations
  expect(f.methods.texSubImage2D).toHaveBeenCalledTimes(1);
  expect(f.methods.drawArrays).toHaveBeenCalledTimes(2);
  expect(f.methods.uniform1f).toHaveBeenCalledWith(expect.anything(), 17 / 1.25);
  gpu.draw(f.video, 276, 450);
  expect(f.methods.texImage2D).toHaveBeenCalledTimes(2);
  expect(f.methods.texSubImage2D).toHaveBeenCalledTimes(2);
  expect(f.methods.getError).toHaveBeenCalledTimes(1);
  gpu.dispose(); gpu.dispose();
  expect(f.methods.deleteTexture).toHaveBeenCalledTimes(2);
  expect(f.methods.deleteShader).toHaveBeenCalledTimes(2);
  expect(f.methods.deleteProgram).toHaveBeenCalledTimes(1);
  expect(f.methods.deleteFramebuffer).toHaveBeenCalledTimes(1);
  expect(f.methods.deleteBuffer).toHaveBeenCalledTimes(1);
  gpu.draw(f.video, 276, 450); expect(f.methods.drawArrays).toHaveBeenCalledTimes(4);
});
it("cleans a partially initialized renderer when shader compilation fails", () => {
  const f = fixture(); vi.mocked(f.gl.getShaderParameter).mockReturnValue(false);
  expect(() => createVerticalPreviewGpu(f.canvas)).toThrow("compilation");
  expect(f.methods.deleteShader).toHaveBeenCalledTimes(1);
});
it("rejects incomplete render targets and propagates denied video uploads", () => {
  const f = fixture(), gpu = createVerticalPreviewGpu(f.canvas);
  vi.mocked(f.gl.checkFramebufferStatus).mockReturnValue(-1);
  expect(() => gpu.draw(f.video, 276, 450)).toThrow("framebuffer"); gpu.dispose();
  const second = fixture(), next = createVerticalPreviewGpu(second.canvas);
  vi.mocked(second.gl.texSubImage2D).mockImplementation(() => { throw new DOMException("denied", "SecurityError"); });
  expect(() => next.draw(second.video, 276, 450)).toThrow("denied"); next.dispose();
});
