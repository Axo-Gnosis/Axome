import type { StippleParams } from './params'

export const VERT_SHADER = `#version 300 es
in vec2 aPos;
void main() {
  gl_Position = vec4(aPos, 0.0, 1.0);
}`

export const FRAG_SHADER = `#version 300 es
precision highp float;

uniform vec2 uInvRes;
uniform float uAspect;
uniform float uCell;
uniform float uBaseMaxR;
uniform float uPhase;
uniform float uSwell;
uniform float uTime;
uniform float uKx;
uniform int uShape;

uniform vec3 uBg;
uniform vec3 uBase;
uniform vec3 uAccent;
uniform float uIntensity;

uniform float uJit;
uniform float uDotTop;
uniform float uDotBot;
uniform float uSizeJit;
uniform float uFloor;
uniform float uPeak;
uniform float uLo;
uniform float uHi;
uniform float uExp;
uniform float uGrow;
uniform float uSoft;
uniform float uPeriod;
uniform float uWaveMix;
uniform float uCrest;
uniform float uShTop;
uniform float uShBot;
uniform float uGlint;
uniform float uColVar;
uniform float uRim;
uniform float uSeed;
uniform float uGrain;
uniform float uVignette;
uniform float uDensOff;
uniform float uTide;

out vec4 outColor;

float h12(vec2 p) {
  vec3 p3 = fract(vec3(p.xyx) * 0.1031);
  p3 += dot(p3, p3.yzx + 33.33);
  return fract((p3.x + p3.y) * p3.z);
}

vec2 h22(vec2 p) {
  vec3 p3 = fract(vec3(p.xyx) * vec3(0.1031, 0.1030, 0.0973));
  p3 += dot(p3, p3.yzx + 33.33);
  return fract((p3.xx + p3.yz) * p3.zy);
}

vec4 h42(vec2 p) {
  vec4 p4 = fract(vec4(p.xyxy) * vec4(0.1031, 0.1030, 0.0973, 0.1099));
  p4 += dot(p4, p4.wzxy + 33.33);
  return fract((p4.xxyz + p4.yzzw) * p4.zywx);
}

// Randomised Bayer rank: finest 2x2 digit is most significant so sparse selections
// stay evenly spaced (blue-noise-like); each block gets a random symmetry.
float rankAt(vec2 c) {
  ivec2 ic = ivec2(c) + ivec2(4096);
  float r = 0.0;
  float w = 0.25;
  for (int k = 0; k < 6; k++) {
    int bx = (ic.x >> k) & 1;
    int by = (ic.y >> k) & 1;
    ivec2 blk = ic >> (k + 1);
    int sym = int(h12(vec2(blk) + vec2(float(k) * 17.31, uSeed * 3.7)) * 8.0);
    if ((sym & 1) == 1) bx = 1 - bx;
    if ((sym & 2) == 2) by = 1 - by;
    if ((sym & 4) == 4) { int t = bx; bx = by; by = t; }
    int idx = (bx == by) ? bx : (bx == 1 ? 2 : 3);
    r += float(idx) * w;
    w *= 0.25;
  }
  return r + h12(c * 1.37 + uSeed) * w * 4.0;
}

float densityAt(float y) {
  float t = smoothstep(uLo, max(uHi, uLo + 1e-3), y);
  return uFloor + (uPeak - uFloor) * pow(t, max(uExp, 0.1));
}

void main() {
  vec2 px = gl_FragCoord.xy;
  vec2 gi = floor(px / uCell);
  float edgeSoft = (uShape == 0) ? uSoft * 1.41421356 : uSoft;
  float maxR = uBaseMaxR + edgeSoft * (uShape == 2 ? 1.41421356 : 1.0);
  float maxR2 = maxR * maxR;

  vec3 col = uBg;
  for (int j = -1; j <= 1; j++) {
    for (int i = -1; i <= 1; i++) {
      vec2 c = gi + vec2(float(i), float(j));
      vec2 jitOff = vec2(0.0);
      if (uJit > 0.0) {
        vec2 h = h22(c + uSeed * 13.1);
        jitOff = (h - 0.5) * uJit;
      }
      vec2 ctr = (c + 0.5 + jitOff) * uCell;
      vec2 delta = px - ctr;
      float d2 = dot(delta, delta);
      if (d2 > maxR2) continue;

      float yT = clamp(1.0 - ctr.y * uInvRes.y, 0.0, 1.0);
      float cellMaxR = 0.5 * uCell * mix(uDotTop, uDotBot, yT) * (1.0 + uSizeJit);
      float boundR = (uShape == 2) ? (cellMaxR + edgeSoft) * 1.41421356 : (cellMaxR + edgeSoft);
      if (d2 > boundR * boundR) continue;

      float dist;
      if (uShape == 0) {
        dist = abs(delta.x) + abs(delta.y);
      } else if (uShape == 1) {
        dist = sqrt(d2);
      } else if (uShape == 2) {
        dist = max(abs(delta.x), abs(delta.y));
      } else {
        vec2 ad = abs(delta);
        dist = max(max(ad.x, ad.y), min(ad.x, ad.y) * 3.0);
      }

      float xw = ctr.x * uKx;
      float roll = 0.65 * sin(xw - uPhase) + 0.35 * sin(xw * 1.73 + 2.0 * uPhase + 1.3);
      float wave = mix(uSwell, roll, uWaveMix);

      float d = densityAt(yT + uDensOff + uTide * wave);

      float r = rankAt(c);
      float w = uGrow * r + 0.004;
      float s = smoothstep(r - w, r + w, d);
      if (s < 0.02) continue;

      vec4 h2 = h42(c + uSeed * 1.7);
      float rad = 0.5 * uCell * mix(uDotTop, uDotBot, yT) * (1.0 + uSizeJit * (h2.x * 2.0 - 1.0)) * s;
      float a = 1.0 - smoothstep(rad - edgeSoft, rad + edgeSoft, dist);
      if (a <= 0.0) continue;

      float pick = floor(h2.y * 3.0);
      float sp = uPeriod * (pick < 1.0 ? 0.25 : (pick < 2.0 ? 0.33333333 : 0.5));
      float sv = sin(6.2831853 * (uTime / max(sp, 0.05) + h2.z));
      float amp = mix(uShTop, uShBot, smoothstep(0.4, 0.9, yT));
      float qd = clamp(dist / max(rad, 1e-3), 0.0, 1.0);
      vec3 dc = uBase * (1.0 + uColVar * (h2.w * 2.0 - 1.0)) * (1.0 - uRim * qd * qd) * (1.0 + amp * sv);
      dc = mix(dc, uAccent, uGlint * pow(max(sv, 0.0), 6.0));

      float band = smoothstep(0.05, 0.4, d) * (1.0 - smoothstep(0.65, 1.1, d));
      float lit = uCrest * smoothstep(0.3, 1.0, wave) * band;
      dc = mix(dc, uAccent * 1.3, lit) * (1.0 + 0.4 * lit);
      col = mix(col, dc, a);
    }
  }

  if (uVignette > 0.0) {
    vec2 v = px * uInvRes - 0.5;
    v.x /= max(uAspect, 1e-4);
    col *= 1.0 - uVignette * smoothstep(0.25, 0.95, length(v) * 1.1);
  }
  if (uGrain > 0.0) {
    float gn = h12(px + floor(uTime * 24.0) * vec2(17.0, 31.0)) - 0.5;
    col += gn * uGrain * 0.3;
  }

  col = mix(uBg, col, uIntensity);
  outColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}`

/** Float uniforms that depend only on params; each converts slider units to shader units. */
export const STIPPLE_UNIFORMS = {
  uIntensity: (p) => p.intensity / 100,
  uJit: (p) => p.jitter / 100,
  uDotTop: (p) => p.dotTop / 100,
  uDotBot: (p) => p.dotBottom / 100,
  uSizeJit: (p) => p.sizeJitter / 100,
  uFloor: (p) => p.topDensity / 1000,
  uPeak: (p) => p.peakDensity / 100,
  uLo: (p) => p.curveStart / 100,
  uHi: (p) => p.curveEnd / 100,
  uExp: (p) => p.curveExp / 100,
  uGrow: (p) => p.growWidth / 100,
  uSoft: (p) => p.edgeSoft / 100,
  uPeriod: (p) => p.tidePeriod / 1000,
  uWaveMix: (p) => p.waveMix / 100,
  uCrest: (p) => p.crest / 100,
  uShTop: (p) => p.shimmerTop / 100,
  uShBot: (p) => p.shimmerBottom / 100,
  uGlint: (p) => p.glint / 100,
  uColVar: (p) => p.colorVar / 100,
  uRim: (p) => p.rimDarken / 100,
  uSeed: (p) => p.seed,
  uGrain: (p) => p.grain / 100,
  uVignette: (p) => p.vignette / 100,
  uDensOff: (p) => (p.density - 50) / 200,
  uTide: (p) => p.tide / 100
} satisfies Record<string, (p: StippleParams) => number>

export type StippleFloatUniform = keyof typeof STIPPLE_UNIFORMS
export const STIPPLE_FLOAT_UNIFORMS = Object.keys(STIPPLE_UNIFORMS) as StippleFloatUniform[]

/** Theme tokens are `#rrggbb` in src/app.css; anything else is unsupported and yields null. */
export function hexToRgb(value: string): [number, number, number] | null {
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(value.trim())
  return m ? [parseInt(m[1], 16) / 255, parseInt(m[2], 16) / 255, parseInt(m[3], 16) / 255] : null
}
