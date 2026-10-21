/** Bounded two-pass GPU backdrop; never reads pixels back to JavaScript. */
export function createVerticalPreviewGpu(canvas: HTMLCanvasElement) {
  const gl = canvas.getContext("webgl", { alpha: false, depth: false, stencil: false, antialias: false, preserveDrawingBuffer: false });
  if (!gl) throw new Error("WebGL unavailable");
  const shaders: WebGLShader[] = [], textures: WebGLTexture[] = [];
  let program: WebGLProgram | null = null, buffer: WebGLBuffer | null = null, framebuffer: WebGLFramebuffer | null = null;
  let disposed = false, configured = false;
  const dispose = () => {
    if (disposed) return;
    disposed = true;
    for (const texture of textures) gl.deleteTexture(texture);
    if (framebuffer) gl.deleteFramebuffer(framebuffer);
    if (buffer) gl.deleteBuffer(buffer);
    if (program) gl.deleteProgram(program);
    for (const shader of shaders) gl.deleteShader(shader);
  };
  try {
    const shader = (type: number, source: string) => {
      const value = gl.createShader(type);
      if (!value) throw new Error("Shader allocation failed");
      shaders.push(value); gl.shaderSource(value, source); gl.compileShader(value);
      if (!gl.getShaderParameter(value, gl.COMPILE_STATUS)) throw new Error("Shader compilation failed");
      return value;
    };
    const vertex = shader(gl.VERTEX_SHADER, `attribute vec2 position; varying vec2 uv;
      void main() { uv = position * .5 + .5; gl_Position = vec4(position, 0., 1.); }`);
    const fragment = shader(gl.FRAGMENT_SHADER, `precision mediump float;
      varying vec2 uv; uniform sampler2D image; uniform vec2 direction;
      uniform float sigma; uniform float colorPass;
      void main() {
        vec3 sum = vec3(0.); float total = 0.;
        vec2 sampleUv = colorPass < .5 ? vec2(uv.x, 1. - uv.y) : uv;
        for (int i = -24; i <= 24; i++) {
          float x = float(i) * .125; float weight = exp(-.5 * x * x);
          sum += texture2D(image, sampleUv + direction * x * sigma).rgb * weight; total += weight;
        }
        vec3 color = sum / total;
        if (colorPass > .5) {
          color *= .48;
          color = mix(vec3(dot(color, vec3(.2126, .7152, .0722))), color, .82);
        }
        gl_FragColor = vec4(color, 1.);
      }`);
    program = gl.createProgram();
    if (!program) throw new Error("Program allocation failed");
    gl.attachShader(program, vertex); gl.attachShader(program, fragment); gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error("Program linking failed");
    gl.useProgram(program);
    buffer = gl.createBuffer(); framebuffer = gl.createFramebuffer();
    if (!buffer || !framebuffer) throw new Error("GPU allocation failed");
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
    const position = gl.getAttribLocation(program, "position");
    if (position < 0) throw new Error("Missing vertex input");
    gl.enableVertexAttribArray(position); gl.vertexAttribPointer(position, 2, gl.FLOAT, false, 0, 0);
    const uniform = (name: string) => {
      const value = gl.getUniformLocation(program!, name);
      if (value === null) throw new Error("Missing shader input");
      return value;
    };
    const direction = uniform("direction"), sigma = uniform("sigma"), colorPass = uniform("colorPass");
    gl.uniform1i(uniform("image"), 0); gl.activeTexture(gl.TEXTURE0);
    for (let i = 0; i < 2; i++) {
      const texture = gl.createTexture();
      if (!texture) throw new Error("Texture allocation failed");
      textures.push(texture); gl.bindTexture(gl.TEXTURE_2D, texture);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    }
    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
    return {
      draw(video: HTMLVideoElement, cssWidth: number, cssHeight: number) {
        if (disposed) return;
        const first = !configured;
        if (first) {
          const scale = Math.min(1, 640 / Math.max(video.videoWidth, video.videoHeight));
          canvas.width = Math.max(1, Math.round(video.videoWidth * scale));
          canvas.height = Math.max(1, Math.round(video.videoHeight * scale));
          gl.bindTexture(gl.TEXTURE_2D, textures[1]);
          gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, canvas.width, canvas.height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
          gl.bindFramebuffer(gl.FRAMEBUFFER, framebuffer);
          gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, textures[1], 0);
          if (gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE) throw new Error("Incomplete framebuffer");
          gl.bindTexture(gl.TEXTURE_2D, textures[0]);
          gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, video.videoWidth, video.videoHeight, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
          configured = true;
        }
        gl.viewport(0, 0, canvas.width, canvas.height);
        gl.bindTexture(gl.TEXTURE_2D, textures[0]);
        gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, gl.RGBA, gl.UNSIGNED_BYTE, video);
        gl.uniform1f(sigma, 17 / Math.max(cssWidth / canvas.width, cssHeight / canvas.height));
        gl.bindFramebuffer(gl.FRAMEBUFFER, framebuffer);
        gl.uniform2f(direction, 1 / canvas.width, 0); gl.uniform1f(colorPass, 0);
        gl.drawArrays(gl.TRIANGLES, 0, 3);
        gl.bindFramebuffer(gl.FRAMEBUFFER, null); gl.bindTexture(gl.TEXTURE_2D, textures[1]);
        gl.uniform2f(direction, 0, 1 / canvas.height); gl.uniform1f(colorPass, 1);
        gl.drawArrays(gl.TRIANGLES, 0, 3);
        if (first && gl.getError() !== gl.NO_ERROR) throw new Error("GPU initialization failed");
      },
      clear() { if (!disposed) { gl.bindFramebuffer(gl.FRAMEBUFFER, null); gl.clearColor(0, 0, 0, 1); gl.clear(gl.COLOR_BUFFER_BIT); } },
      dispose,
    };
  } catch (error) { dispose(); throw error; }
}
