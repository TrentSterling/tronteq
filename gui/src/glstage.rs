//! The GL viz stage — real shaders under the canvas. Ping-pong feedback
//! rendertextures (the Milkdrop trick), FFT buckets + waveform + a scrolling
//! spectrum-HISTORY texture uploaded to the GPU, VizBus stats as uniforms.
//! Painter layers + the EQ curve composite over the result.
//!
//! Renders through egui's PaintCallback on the glow backend (GL 3.3 core).
//! VIEWPORT DISCIPLINE (learned the hard way — v0.6.0 painted over the
//! panels): egui_glow sets the viewport to the callback rect and scissor to
//! the clip rect BEFORE invoking us. The offscreen FBO passes disable scissor
//! and use their own viewport; the present pass restores egui's viewport,
//! re-enables scissor, and draws a plain fullscreen triangle — which then
//! fills exactly the canvas rect and clips exactly to egui's clip. Never
//! reproject the rect into window NDC by hand.

use std::time::Instant;

use eframe::glow::{self, HasContext};

/// Spectrum texture width (matches the analyzer's bin count upper bound).
pub const SPEC_W: usize = 64;
/// Waveform texture width (decimated).
pub const WAVE_W: usize = 256;
/// Spectrum-history rows (the audio terrain's depth).
pub const HIST_ROWS: usize = 128;

/// Everything a shader frame needs, by value (Copy) so the paint callback can
/// own it without borrowing App.
#[derive(Clone, Copy)]
pub struct Uniforms {
    pub mode: GlMode,
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    pub pulse: f32,
    pub beat_phase: f32,
    pub bright: f32,
    pub rainbow: bool,
    pub accent: [f32; 3],
    pub spec: [f32; SPEC_W],
    pub wave: [f32; WAVE_W],
    pub bpm: f32,
    pub bpm_conf: f32,
    pub flux: f32,
    pub loud: f32,
    pub crest: f32,
    pub corr: f32,
    pub width: f32,
    /// Post-fx bitmask (0 = off). Applied as a stackable overlay pass AFTER
    /// the mode's feedback pass, over the feedback loop's output — the
    /// feedback texture itself never sees post-fx (would corrupt the trail).
    pub fx: u32,
}

impl Default for Uniforms {
    fn default() -> Self {
        Uniforms {
            mode: GlMode::Warp,
            bass: 0.0,
            mid: 0.0,
            treble: 0.0,
            pulse: 0.0,
            beat_phase: 0.0,
            bright: 0.0,
            rainbow: true,
            accent: [0.0, 0.88, 1.0],
            spec: [0.0; SPEC_W],
            wave: [0.0; WAVE_W],
            bpm: 0.0,
            bpm_conf: 0.0,
            flux: 0.0,
            loud: 0.0,
            crest: 0.0,
            corr: 0.0,
            width: 0.0,
            fx: 0,
        }
    }
}

/// One variant per fragment program, in `PROGRAM_SRC` order.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GlMode {
    Warp,
    Flame,
    Smoke,
    Plasma,
    Starfield,
    Kaleido,
    Tunnel3d,
    Metaballs,
    Voronoi,
    Nebula,
    Terrain,
    Ripples,
    Julia,
    Matrix,
    ScopeRing,
    Skybox,
    Aurora,
    Outrun,
    City,
    Wormhole,
    Spiro,
    Laser,
    DiscoBall,
    HexGrid,
    Lightning,
    Dna,
    Bubbles,
    Copper,
    LedWall,
    Sonar,
    Pulsar,
    BlackHole,
    Ocean,
}

pub const MODE_COUNT: usize = 33;

/// Stackable post-fx bits for `Uniforms::fx` — mix and match over ANY base
/// GL mode. Kept as plain constants (not an enum) since any combination is
/// valid at once.
pub const FX_MIRROR: u32 = 1 << 0;
pub const FX_ZOOMBLUR: u32 = 1 << 1;
pub const FX_ABERRATION: u32 = 1 << 2;
pub const FX_PIXELATE: u32 = 1 << 3;
pub const FX_HALFTONE: u32 = 1 << 4;
pub const FX_SCANLINES: u32 = 1 << 5;
pub const FX_GRAIN: u32 = 1 << 6;
pub const FX_STROBE: u32 = 1 << 7;
pub const FX_EDGEGLOW: u32 = 1 << 8;
pub const FX_THERMAL: u32 = 1 << 9;

/// Owns GL resource ids only (the context is passed into every call, keeping
/// this Send for the paint callback's Arc<Mutex>).
pub struct GlStage {
    prog: [glow::Program; MODE_COUNT],
    present: glow::Program,
    post: glow::Program,
    vao: glow::VertexArray,
    fbo: [glow::Framebuffer; 2],
    tex: [glow::Texture; 2],
    /// Third color target: the stackable post-fx pass renders here from the
    /// mode's feedback output, kept fully separate from the ping-pong pair
    /// so post-fx NEVER re-enters the feedback loop.
    post_fbo: glow::Framebuffer,
    post_tex: glow::Texture,
    size: (i32, i32),
    spec_tex: glow::Texture,
    wave_tex: glow::Texture,
    hist_tex: glow::Texture,
    hist_row: usize,
    frame: u64,
    ping: usize,
    /// Simulation clock: advanced by CLAMPED real dt per painted frame, so a
    /// stalled frame never fast-forwards the effects on resume (backlog is
    /// dropped, not replayed — same rule as the painter histories).
    sim: f32,
    last_paint: Option<Instant>,
}

const VS_FULL: &str = r#"#version 330 core
const vec2 P[3] = vec2[3](vec2(-1.,-1.), vec2(3.,-1.), vec2(-1.,3.));
out vec2 vUv;
void main() {
    vec2 p = P[gl_VertexID];
    vUv = p * 0.5 + 0.5;
    gl_Position = vec4(p, 0., 1.);
}"#;

/// Shared fragment prelude: uniforms + helpers every mode uses.
const FRAG_COMMON: &str = r#"#version 330 core
in vec2 vUv;
out vec4 frag;
uniform sampler2D uPrev;
uniform sampler2D uSpec;
uniform sampler2D uWave;
uniform sampler2D uHist;
uniform float uHistRow;
uniform vec2  uRes;
uniform float uTime;
uniform float uBass;
uniform float uMid;
uniform float uTreb;
uniform float uPulse;
uniform float uPhase;
uniform float uBright;
uniform int   uRainbow;
uniform vec3  uAccent;
uniform float uBpm;
uniform float uBpmConf;
uniform float uFlux;
uniform float uLoud;
uniform float uCrest;
uniform float uCorr;
uniform float uWidth;

float spec(float x)  { return texture(uSpec, vec2(clamp(x, 0., 1.), .5)).r; }
float wav(float x)   { return texture(uWave, vec2(clamp(x, 0., 1.), .5)).r; }
float hist(vec2 xz)  { return texture(uHist, vec2(clamp(xz.x, 0., 1.), fract(uHistRow - xz.y))).r; }

vec3 hsv2rgb(vec3 c) {
    vec3 p = abs(fract(c.xxx + vec3(0., 2./3., 1./3.)) * 6. - 3.);
    return c.z * mix(vec3(1.), clamp(p - 1., 0., 1.), c.y);
}
float hash(vec2 p) {
    p = fract(p * vec2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}
float vnoise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    f = f * f * (3. - 2. * f);
    return mix(mix(hash(i), hash(i + vec2(1., 0.)), f.x),
               mix(hash(i + vec2(0., 1.)), hash(i + vec2(1., 1.)), f.x), f.y);
}
float fbm(vec2 p) {
    float v = 0., a = .5;
    for (int i = 0; i < 5; i++) { v += a * vnoise(p); p = p * 2.03 + vec2(1.7, 4.1); a *= .5; }
    return v;
}
vec2 aspect(vec2 uv) { return (uv - .5) * vec2(uRes.x / uRes.y, 1.); }
"#;

const FRAG_WARP: &str = r#"
void main() {
    vec2 c = aspect(vUv);
    float ang = .0035 + uBass * .028 + sin(uTime * .13) * .004;
    float zoom = .994 - uPulse * .012;
    float ca = cos(ang), sa = sin(ang);
    vec2 w = mat2(ca, -sa, sa, ca) * c * zoom;
    w.x /= uRes.x / uRes.y;
    vec3 prev = texture(uPrev, w + .5).rgb;
    prev *= .962 + uTreb * .022;
    prev = min(prev, vec3(1.2));
    float r = length(c) * 2.;
    float a = atan(c.y, c.x) / 6.28318 + .5;
    float s = spec(fract(a + uPhase));
    float ring = smoothstep(.045, .0, abs(r - (.38 + s * .5 + uPulse * .06)));
    float w0 = wav(vUv.x) * .5;
    float wv = smoothstep(.02, .0, abs(vUv.y - .5 - w0 * .35));
    vec3 base = (uRainbow == 1) ? hsv2rgb(vec3(a + uTime * .02, .85, 1.)) : uAccent;
    vec3 add = base * (ring * (.55 + uPulse * .9) + wv * .18);
    frag = vec4(max(prev, add), 1.);
}"#;

const FRAG_FLAME: &str = r#"
// Same bottom-fed spectrum fire + alpha-as-heat feedback structure, made
// FIERCE: stronger upward advection plus sideways curl turbulence for
// licking, curling tongues (not a straight vertical smear); a multi-stop
// heat-shaping curve that punches a hot white-yellow-orange-red core instead
// of a flat linear ramp; ember sparks that shoot up fast and flare on
// uPulse; uLoud raises the whole fire's baseline. uRainbow=1 hue-cycles the
// fire into a plasma flame; uRainbow=0 tints with uAccent, with the hottest
// pixels still bleeding through toward white.

float mr_heatShape(float h) {
    h = clamp(h, 0.0, 1.0);
    float s = smoothstep(0.00, 0.35, h) * 0.35;
    s += smoothstep(0.30, 0.65, h) * 0.35;
    s += smoothstep(0.60, 0.90, h) * 0.45;
    s += smoothstep(0.88, 1.05, h) * 0.55;
    return s; // fierce multi-stop energy curve — punches brighter than h*k
}

vec2 mr_curl(vec2 p) {
    float e = 0.015;
    float n1 = vnoise(p + vec2(0.0, e));
    float n2 = vnoise(p - vec2(0.0, e));
    float n3 = vnoise(p + vec2(e, 0.0));
    float n4 = vnoise(p - vec2(e, 0.0));
    return vec2(n1 - n2, n4 - n3) / (2.0 * e);
}

void main() {
    vec2 px = 1.0 / uRes;
    float loud = uLoud;

    // Sideways curl turbulence: licking, curling tongues instead of a
    // straight vertical smear.
    vec2 swirl = mr_curl(vUv * 5.0 + vec2(0.0, -uTime * 0.5))
               * (0.010 + uMid * 0.020 + loud * 0.012);
    float drift = (vnoise(vUv * 10.0 + uTime * 1.1) - 0.5) * 4.5 * px.x;
    // Stronger upward advection = taller tongues; loudness lifts them further.
    float lift = (2.4 + uBass * 3.6 + loud * 2.0) * px.y;
    vec2 below = vUv + vec2(drift, -lift) + swirl;
    float heat = texture(uPrev, below).a;
    heat *= 0.984 - vnoise(vUv * 14.0 - uTime * 1.9) * 0.05;

    // Base feed row rises with loudness: a loud mix keeps the whole fire hot.
    float baseY = 0.05 + loud * 0.05;
    float src = smoothstep(baseY, 0.0, vUv.y) * spec(vUv.x) * (1.0 + uPulse * 2.0 + loud * 0.6);
    float mid = smoothstep(0.14, 0.0, distance(vUv, vec2(0.5, 0.05))) * uPulse * 1.1;
    heat = max(heat, clamp(src + mid, 0.0, 1.3));

    // Ember sparks: small bright cells shooting up fast, flaring on uPulse.
    vec2 sp = vUv * vec2(24.0, 10.0);
    sp.y += uTime * (2.2 + uPulse * 5.0);
    vec2 sid = floor(sp) + floor(uTime * 0.6);
    float sh = hash(sid);
    float sparkLife = fract(sp.y);
    float sparkOn = step(0.92, sh) * (0.4 + uPulse);
    float spark = smoothstep(0.28, 0.0, length(fract(sp) - 0.5))
                * sparkOn * smoothstep(1.0, 0.55, sparkLife);
    heat = clamp(heat + spark * 0.8, 0.0, 1.4);

    float e = mr_heatShape(heat);
    vec3 col;
    if (uRainbow == 1) {
        // Hue-cycled plasma fire: color cycles with height + time + heat
        // instead of sitting on a fixed fire palette.
        col = hsv2rgb(vec3(fract(uTime * 0.10 + vUv.y * 0.6 + heat * 0.35), 0.85, clamp(e, 0.0, 1.0)));
    } else {
        // uAccent tint, with the white-hot core bleeding through at peak heat.
        col = uAccent * e;
        col = mix(col, vec3(1.0), smoothstep(0.86, 1.05, heat) * 0.7);
    }
    frag = vec4(col, clamp(heat, 0.0, 1.0));
}"#;

const FRAG_SMOKE: &str = r#"
vec2 curl(vec2 p) {
    float e = .01;
    return vec2(vnoise(p + vec2(0., e)) - vnoise(p - vec2(0., e)),
                vnoise(p - vec2(e, 0.)) - vnoise(p + vec2(e, 0.))) / (2. * e);
}
void main() {
    vec2 flow = curl(vUv * 3.2 + vec2(0., uTime * .06)) * (.0016 + uMid * .0045);
    flow.y -= .0014 + uBass * .0028;
    float d = texture(uPrev, vUv + flow).a;
    d *= .988;
    float floorSrc = smoothstep(.06, .0, vUv.y) * spec(vUv.x) * (.5 + uBass);
    float burst = smoothstep(.16, .0, distance(vUv, vec2(.5))) * uPulse * .9;
    d = clamp(max(d, floorSrc + burst), 0., 1.);
    float shade = d * (.55 + .45 * vnoise(vUv * 6. + uTime * .12));
    vec3 tint = (uRainbow == 1)
        ? hsv2rgb(vec3(.55 + uBright * .35 + vUv.y * .12, .55, shade))
        : uAccent * shade;
    frag = vec4(tint, d);
}"#;

const FRAG_PLASMA: &str = r#"
void main() {
    vec2 p = aspect(vUv);
    float t = uTime * .6;
    float k = 3. + uBass * 4.;
    float s = sin(p.x * k + t) + sin(p.y * k * 1.3 - t * 1.2)
            + sin((p.x + p.y) * k * .8 + t * .7) + sin(length(p) * k * 1.7 - t * 1.5);
    s = s * .25 + .5;
    float e = spec(fract(s + uPhase * .5));
    vec3 col = (uRainbow == 1) ? hsv2rgb(vec3(s + uTime * .03, .8, .55 + .45 * e))
                               : uAccent * (s * .6 + e * .6);
    frag = vec4(col * (.8 + uPulse * .5), 0.);
}"#;

const FRAG_STARFIELD: &str = r#"
// Layered screen-space starfield: 6 depth layers, each one grid-cell lookup
// per pixel (NOT a per-star loop) so hundreds of stars are on screen from a
// 6-iteration outer loop. Each layer's screen scale is driven by an
// exponential zoom cycle (perspective divide) so stars are born small near
// the vanishing point and rush outward as their layer's cycle advances,
// which reads as flying toward the viewer. uPulse spikes the zoom for a
// hyperspace lurch, uBass elongates stars into radial warp-streaks, uTreb
// drives twinkle, and a rare hashed "giant" per cell gets a saturated color.

float mr_starHash(vec2 id, float layer) { return hash(id + layer * 17.13); }

vec3 mr_starLayer(vec2 p, float fi, float t, float bass, float treb, float pulse, int rainbow, vec3 accent) {
    vec3 col = vec3(0.0);
    float seed = hash(vec2(fi, 4.7));
    float speed = 0.045 + bass * 0.30 + pulse * 0.55;
    float cycle = fract(t * speed + seed);
    // Hyperspace lurch: a strong beat snaps every layer outward at once.
    float zoom = exp2(cycle * 5.0) * (1.0 + pulse * pulse * 2.2);
    vec2 pos = p / zoom;
    float grid = 5.0 + fi * 2.5;
    vec2 gp = pos * grid;
    vec2 id = floor(gp);
    vec2 gv = fract(gp) - 0.5;
    float h = mr_starHash(id, fi);
    if (h < 0.42) return col;                 // ~58% of cells hold a star
    float starR = mix(0.05, 0.16, fract(h * 13.1));
    // Radial warp-streak: elongate the star along the direction from center.
    vec2 dir = normalize(pos + vec2(1.0e-4));
    float along = dot(gv, dir);
    float across = dot(gv, vec2(-dir.y, dir.x));
    float stretch = 1.0 + bass * 7.0 + pulse * 3.0;
    float dist = length(vec2(along / stretch, across));
    float core = smoothstep(starR, starR * 0.15, dist);
    float tw = 0.6 + 0.4 * sin(t * (3.0 + treb * 9.0) + h * 46.0);
    // Fade both ends of the depth cycle so spawn/reset never pops.
    float fade = smoothstep(0.0, 0.10, cycle) * smoothstep(1.0, 0.82, cycle);
    float bright = core * mix(0.7, 1.0, tw) * fade;
    bool giant = fract(h * 91.7) > 0.965;
    vec3 starCol = (rainbow == 1)
        ? (giant ? hsv2rgb(vec3(fract(h * 53.0), 0.65, 1.0)) : vec3(1.0, 0.97, 0.9))
        : (giant ? accent : vec3(1.0));
    col = starCol * bright * (giant ? 1.8 : 1.0) * (starR / 0.16);
    return col;
}

void main() {
    vec2 p = aspect(vUv) * 2.0;
    vec3 col = vec3(0.0);
    for (int i = 0; i < 6; i++) {
        float fi = float(i);
        col += mr_starLayer(p, fi, uTime, uBass, uTreb, uPulse, uRainbow, uAccent);
    }
    // Subtle feedback trail, biased down hard so it can never bloom white.
    vec2 trailUv = 0.5 + (vUv - 0.5) * (0.988 - uBass * 0.006);
    vec3 prevCol = max(texture(uPrev, trailUv).rgb * (0.86 - uPulse * 0.05) - 0.006, 0.0);
    col = max(col, prevCol);
    frag = vec4(min(col, vec3(1.3)), 0.0);
}"#;

const FRAG_KALEIDO: &str = r#"
void main() {
    vec2 p = aspect(vUv);
    float seg = 6. + floor(uBright * 6.);
    float a = atan(p.y, p.x) + uTime * .05 + uPulse * .1;
    float r = length(p);
    a = abs(mod(a, 6.28318 / seg) - 3.14159 / seg);
    vec2 q = vec2(cos(a), sin(a)) * r;
    q.x /= uRes.x / uRes.y;
    vec3 prev = texture(uPrev, q * .92 + .5).rgb * .96;
    float ring = smoothstep(.03, .0, abs(r - .25 - spec(fract(r + uPhase)) * .35));
    vec3 base = (uRainbow == 1) ? hsv2rgb(vec3(r + uTime * .04, .85, 1.)) : uAccent;
    frag = vec4(max(prev, base * ring * (.5 + uPulse)), 0.);
}"#;

const FRAG_TUNNEL3D: &str = r#"
void main() {
    vec2 p = aspect(vUv);
    float r = length(p) + 1e-4;
    float a = atan(p.y, p.x) / 6.28318 + .5;
    float z = .25 / r + uTime * (1.2 + uBass * 3.);
    float stripes = spec(fract(a * 3.));
    float rings = smoothstep(.4, .9, sin(z * 6.283) * .5 + .5);
    float wall = stripes * .8 + rings * .4;
    vec3 base = (uRainbow == 1) ? hsv2rgb(vec3(fract(a + z * .05), .8, 1.)) : uAccent;
    vec3 col = base * wall * smoothstep(.0, .35, r);
    frag = vec4(col * (.7 + uPulse * .6), 0.);
}"#;

const FRAG_METABALLS: &str = r#"
void main() {
    vec2 p = aspect(vUv);
    float f = 0.;
    vec3 col = vec3(0.);
    float e0 = uBass, e1 = uMid, e2 = uTreb, e3 = uPulse, e4 = uBright, e5 = uBass * .5 + uTreb * .5;
    for (int i = 0; i < 6; i++) {
        float fi = float(i);
        float en = (i == 0) ? e0 : (i == 1) ? e1 : (i == 2) ? e2 : (i == 3) ? e3 : (i == 4) ? e4 : e5;
        vec2 c = vec2(sin(uTime * (.3 + fi * .11) + fi * 2.1),
                      cos(uTime * (.23 + fi * .07) + fi * 1.3)) * .32;
        float rr = .02 + en * .06;
        float d = dot(p - c, p - c);
        float g = rr / max(d, 1e-5);
        f += g;
        col += ((uRainbow == 1) ? hsv2rgb(vec3(fi / 6. + uTime * .02, .8, 1.)) : uAccent) * g;
    }
    float m = smoothstep(1., 1.6, f);
    frag = vec4(col / max(f, 1e-3) * m + col * .06, 0.);
}"#;

const FRAG_VORONOI: &str = r#"
void main() {
    vec2 p = vUv * vec2(uRes.x / uRes.y, 1.) * 6.;
    p.y += uTime * .4;
    vec2 ic = floor(p), fpt = fract(p);
    float f1 = 9.;
    vec2 id = ic;
    for (int y = -1; y <= 1; y++) for (int x = -1; x <= 1; x++) {
        vec2 g = vec2(float(x), float(y));
        vec2 o = .5 + .4 * sin(uTime * .8 + 6.28 * vec2(hash(ic + g), hash(ic + g + vec2(7., 3.))));
        float d = length(g + o - fpt);
        if (d < f1) { f1 = d; id = ic + g; }
    }
    float e = spec(fract(hash(id) * .9));
    vec3 base = (uRainbow == 1) ? hsv2rgb(vec3(hash(id) + uTime * .01, .75, .35 + .65 * e))
                                : uAccent * (.2 + e);
    float edge = smoothstep(.08, .02, f1) * uPulse;
    frag = vec4(base + vec3(edge), 0.);
}"#;

const FRAG_NEBULA: &str = r#"
void main() {
    vec2 p = aspect(vUv) * 2.;
    float t = uTime * .03;
    float n1 = fbm(p * 1.5 + vec2(t, -t * .7));
    float n2 = fbm(p * 3. + vec2(-t * 1.3, t));
    float d = n1 * n2 * 1.8;
    float core = smoothstep(.35, .9, d) * (.6 + uBass);
    vec3 c1 = (uRainbow == 1) ? hsv2rgb(vec3(.68 + uBright * .2, .7, 1.)) : uAccent;
    vec3 c2 = (uRainbow == 1) ? hsv2rgb(vec3(.95, .8, 1.)) : uAccent * .6;
    vec3 col = c1 * d * .5 + c2 * core;
    vec3 prev = texture(uPrev, vUv + vec2(n1 - n2) * .002).rgb;
    frag = vec4(max(col, prev * .94), 0.);
}"#;

/// The audio terrain: tron wireframe mountains raymarched over the REAL
/// spectrum history texture (x = frequency, depth = time).
const FRAG_TERRAIN: &str = r#"
float hgt(vec2 xz) {
    float h = hist(vec2(xz.x * .5 + .5, xz.y * (1. / 16.)));
    // Camera flies at y=.58: keep peaks safely below the eye so rays get
    // DEPTH before hitting (v0.7.0 spawned the camera inside the terrain
    // and every pixel hit at t~0 - a flat purple wash).
    return h * (.30 + uPulse * .10);
}
void main() {
    vec2 p = aspect(vUv);
    vec3 ro = vec3(0., .58, 0.);
    vec3 rd = normalize(vec3(p.x, p.y - .18, .8));
    float scroll = uTime * (1.2 + uBass * 2.);
    vec3 col = vec3(0.);
    float glow = smoothstep(.22, .0, abs(rd.y + .04)) * (.25 + uTreb * .5);
    vec3 horizon = (uRainbow == 1) ? hsv2rgb(vec3(uPhase, .7, 1.)) : uAccent;
    col += horizon * glow;
    if (rd.y < -.01) {
        float tt = .15;
        bool hit = false;
        vec3 pos = ro;
        for (int i = 0; i < 64; i++) {
            pos = ro + rd * tt;
            if (pos.y < hgt(vec2(pos.x, pos.z + scroll))) { hit = true; break; }
            tt += .03 + tt * .05;
            if (tt > 8.) break;
        }
        if (hit) {
            float zc = pos.z + scroll;
            vec2 g = vec2(pos.x * 5., zc * 5.);
            vec2 gf = abs(fract(g) - .5);
            float line = smoothstep(.40, .5, max(gf.x, gf.y));
            float h = hgt(vec2(pos.x, zc));
            float fog = exp(-tt * .45);
            vec3 wire = (uRainbow == 1) ? hsv2rgb(vec3(fract(h * 2.2 + uTime * .02), .85, 1.)) : uAccent;
            col = mix(col, wire * (line * 1.35 + h * 1.4) + vec3(.010), fog);
        }
    }
    frag = vec4(col, 0.);
}"#;

const FRAG_RIPPLES: &str = r#"
void main() {
    vec2 p = aspect(vUv);
    float r = length(p);
    float ring = 0.;
    for (int k = 0; k < 3; k++) {
        float ph = fract(uPhase + float(k) / 3.);
        ring += smoothstep(.05, .0, abs(r - ph * .75)) * (1. - ph);
    }
    vec2 n = p / max(r, 1e-4);
    vec3 prev = texture(uPrev, vUv - n * ring * .02).rgb * .965;
    vec3 base = (uRainbow == 1) ? hsv2rgb(vec3(r - uTime * .05, .7, 1.)) : uAccent;
    frag = vec4(max(prev, base * ring * (.5 + uBass)), 0.);
}"#;

const FRAG_JULIA: &str = r#"
// Same c-orbit morph + bass push as before, now riding a slow LOG-SCALE zoom
// cycle that breathes between the full overview and ~8x deep into a
// filament-rich edge region (log-interpolated scale + a pan toward the
// target, so the zoom feels smooth and continuous, not linear). Escape-time
// coloring is the standard smooth/continuous (log-log) formula so rings
// never band even deep in the zoom. A real distance-estimate rim glow (from
// tracking dz alongside z) is normalized by the current view scale so the
// boundary stays a consistent screen-thickness across the whole zoom range.
// uPhase shifts the palette hue. Iteration count is capped at 80 per the
// shader-loop contract (asked for ~90; the continuous coloring below reads
// identically smooth at 80 - there's no banding either way).
void main() {
    float t = uTime;

    // Slow smooth cycle 0..1: 0 = full overview, 1 = ~8x deep on the target.
    float cyc = 0.5 - 0.5 * cos(t * (6.28318 / 52.0));
    vec2 target = vec2(0.318, 0.412); // filament-rich edge of this c's Julia set
    float baseScale = 2.4 - uPulse * 0.5 - uBass * 0.25;
    float scale = exp(mix(log(baseScale), log(baseScale / 8.0), cyc));
    vec2 center = mix(vec2(0.0), target, cyc);

    vec2 raw = aspect(vUv) * scale;
    float ra = t * 0.05;
    raw = mat2(cos(ra), -sin(ra), sin(ra), cos(ra)) * raw;
    vec2 p = raw + center;

    // The c-orbit is what MORPHS the set: main orbit, a shimmer epicycle,
    // and the audio pushing c directly.
    vec2 c = vec2(-.745, .186)
           + vec2(sin(t * .37), cos(t * .29)) * .085
           + vec2(sin(t * 1.7), cos(t * 2.3)) * .012 * uMid
           + vec2(uBass * .06, uPulse * .03);

    vec2 z = p;
    vec2 dz = vec2(1.0, 0.0);
    float it = 0.0;
    bool esc = false;
    const int MAXI = 80;
    for (int i = 0; i < MAXI; i++) {
        dz = 2.0 * vec2(z.x * dz.x - z.y * dz.y, z.x * dz.y + z.y * dz.x);
        dz = clamp(dz, vec2(-1.0e6), vec2(1.0e6));
        z = vec2(z.x * z.x - z.y * z.y, 2.0 * z.x * z.y) + c;
        if (dot(z, z) > 64.0) { it = float(i); esc = true; break; }
    }

    // Smooth (log-log) escape-time coloring: continuous, so no ring banding
    // even at deep zoom where a whole ring would otherwise span one pixel.
    float fCol;
    if (esc) {
        float logZn = log(dot(z, z)) * 0.5;
        float nu = log(logZn / log(2.0)) / log(2.0);
        float smoothIt = float(it) + 1.0 - nu;
        fCol = clamp(smoothIt / float(MAXI), 0.0, 1.0);
    } else {
        fCol = 0.03; // interior stays near-black
    }

    // Distance-estimate boundary glow, normalized by the current view scale
    // so the rim stays roughly the same screen-thickness whether we're at
    // the overview or ~8x deep.
    float zlen = length(z);
    float dlen = max(length(dz), 1.0e-6);
    float de = 0.5 * zlen * log(max(zlen, 1.0001)) / dlen;
    float glow = clamp(exp(-de * 55.0 / scale), 0.0, 1.0);

    vec3 base = (uRainbow == 1)
        ? hsv2rgb(vec3(fCol * .85 + t * .05 + uPhase * .4, .82, clamp(fCol * 1.15, 0., 1.)))
        : uAccent * clamp(fCol * 1.15, 0., 1.2);
    vec3 rim = (uRainbow == 1)
        ? hsv2rgb(vec3(fract(uPhase * .6 + .55), .55, 1.))
        : uAccent;
    vec3 col = base + rim * glow * .85;
    frag = vec4(col * (.8 + uPulse * .6), 0.);
}"#;

const FRAG_MATRIX: &str = r#"
// Rainbow matrix rain, hero piece. Vertical columns of hash-based blocky
// glyphs fall down a fixed pixel-cell grid; a bright white-hot head redraws
// each frame while it dwells in a row, and once it moves on the trail is
// pure phosphor decay through the uPrev feedback below (bias strictly down
// per the no-blowout rule). uPulse spawns bursts of fresh cascading heads,
// uBass + the locked BPM drive fall speed, uTreb speeds up glyph mutation
// into a flicker, uLoud thickens the storm, uMid lifts body brightness.

void main() {
    vec2 res = uRes;
    float cellPx = 15.0;
    vec2 grid = max(res / cellPx, vec2(4.0));
    vec2 cellUv = vUv * grid;
    float col = floor(cellUv.x);
    float rowIdx = floor(cellUv.y);
    vec2 cellFrac = fract(cellUv);
    vec2 cellId = vec2(col, rowIdx);

    float colH = hash(vec2(col, 17.3));
    float colH2 = hash(vec2(col, 41.7));

    // Fraction of columns actually raining; uLoud thickens the storm.
    float density = 0.42 + uLoud * 0.42;
    float colActive = step(1.0 - density, hash(vec2(col, 5.0)));

    // Fall speed rides bass energy plus the locked BPM (falls back to a
    // fixed rate when unlocked, so it still animates in silence).
    float bpmRate = uBpm > 0.0 ? uBpm / 60.0 : 1.8;
    float speed = (4.0 + uBass * 11.0) * (0.6 + bpmRate * 0.5);
    float cyc = grid.y + 10.0;
    float travel = uTime * speed + colH * 137.0;
    float headRow = (grid.y - 1.0) - mod(travel, cyc);
    float isHead = step(abs(rowIdx - headRow), 0.5) * colActive;

    // Strong beat pulse: extra columns light up with fresh cascading heads.
    float beatId = floor(uTime * bpmRate);
    float burstAmt = smoothstep(0.6, 1.0, uPulse);
    float burstGate = step(0.72, hash(vec2(col, beatId)));
    float burstHead = burstGate * burstAmt * step(abs(rowIdx - headRow), 1.5);
    float burstRow2 = (grid.y - 1.0) - mod(travel * 1.7 + colH2 * 50.0, cyc * 0.6);
    float burstHead2 = burstGate * burstAmt * step(abs(rowIdx - burstRow2), 0.5);

    float head = clamp(isHead + burstHead + burstHead2, 0.0, 1.0);

    // Hash-based blocky glyph inside the cell, mutated a few times a second;
    // treble speeds the mutation rate up into a flicker.
    float mutateRate = 2.0 + uTreb * 10.0;
    float glyphTick = floor(uTime * mutateRate + colH * 13.0);
    vec2 sub = floor(cellFrac * 4.0);
    float gseed = hash(cellId * 1.37 + sub * 0.53 + glyphTick * 0.91);
    float glyphOn = step(0.5, gseed);
    vec2 m = abs(cellFrac - 0.5);
    float border = step(m.x, 0.40) * step(m.y, 0.44);
    glyphOn *= border;

    // Sparse extra twinkle across the whole field, driven by treble.
    float flickTick = floor(uTime * 18.0);
    float flickChance = uTreb * 0.12;
    float flickGate = step(1.0 - flickChance, hash(cellId + flickTick * 3.7));

    float hueBase = colH + uTime * 0.02 + uPhase * 0.05;
    vec3 tint = (uRainbow == 1) ? hsv2rgb(vec3(hueBase, 0.85, 1.0)) : uAccent;

    float bodyLevel = (0.45 + 0.25 * uMid) * colActive;
    float glow = clamp(head + flickGate * 0.4 * (1.0 - head), 0.0, 1.0);
    vec3 srcCol = tint * mix(bodyLevel, 1.0, glow) * glow;
    srcCol = mix(srcCol, vec3(1.0), head * 0.85);
    srcCol *= glyphOn;

    vec3 prev = texture(uPrev, vUv).rgb;
    prev = prev * (0.955 - uTreb * 0.01);
    prev = max(prev - 0.004, vec3(0.0));

    vec3 outCol = max(prev, srcCol);
    outCol += burstAmt * 0.05 * tint;

    frag = vec4(min(outCol, vec3(1.2)), 0.0);
}"#;

const FRAG_SCOPERING: &str = r#"
// Circular oscilloscope. wav() is wrapped around a ring (radius = base +
// wave amplitude), drawn as a glowing line via distance-to-curve smoothstep.
// A second inner ring mixes two phase-offset wave taps: uCorr morphs it
// between a plain circle (mono, correlated channels) and a crossed two-lobe
// figure (wide/decorrelated stereo), with uWidth adding extra bulge to the
// crossed lobes. Phosphor persistence comes from a rotating, slowly
// zooming resample of uPrev, biased strictly down so it can never blow out.

void main() {
    vec2 c = aspect(vUv);
    float r = length(c);
    float ang = atan(c.y, c.x);
    float a01 = fract(ang / 6.28318 + 0.5);

    // Outer ring: breathes on bass, sweeps its sample point with the beat
    // phase so the waveform bumps visibly travel around the circle.
    float sampleAng = fract(a01 + uPhase * 0.5);
    float wv = wav(sampleAng);
    float baseR = 0.30 + uBass * 0.10;
    float ringR = baseR + wv * 0.16;
    float lineW = 0.007 + uPulse * 0.005;
    float ringGlow = smoothstep(lineW * 2.6, 0.0, abs(r - ringR));

    float hue = a01 + uTime * 0.05;
    vec3 ringCol = (uRainbow == 1) ? hsv2rgb(vec3(hue, 0.85, 1.0)) : uAccent;

    // Inner ring: mixes two wave taps at offset phase. uCorr near 1 (mono)
    // collapses it to a plain circle; low/negative correlation (wide
    // stereo) crosses it into a two-lobed figure, uWidth adds bulge.
    float wideAmt = clamp(0.5 - uCorr * 0.5, 0.0, 1.0);
    float innerBase = 0.14 + uBass * 0.05;
    float wvA = wav(sampleAng);
    float wvB = wav(fract(sampleAng + 0.5));
    float monoR = wvA * 0.07;
    float crossMod = sin(a01 * 6.28318 * 2.0 - uTime * 0.4) * 0.07 * (0.5 + uWidth * 0.5);
    float crossR = (wvA - wvB) * 0.05 + crossMod;
    float innerR = innerBase + mix(monoR, crossR, wideAmt);
    float innerGlow = smoothstep(lineW * 2.2, 0.0, abs(r - innerR));
    vec3 innerCol = (uRainbow == 1) ? hsv2rgb(vec3(hue + 0.5, 0.85, 1.0)) : uAccent * 0.75;

    // Phosphor trail: rotate + zoom the previous frame, bias strictly down.
    float fRot = 0.010 + uBass * 0.012;
    float fZoom = 0.985 - uPulse * 0.010;
    float fca = cos(fRot);
    float fsa = sin(fRot);
    vec2 fb = mat2(fca, -fsa, fsa, fca) * c * fZoom;
    fb.x /= uRes.x / uRes.y;
    vec3 prev = texture(uPrev, fb + 0.5).rgb;
    prev = prev * (0.965 - uTreb * 0.01);
    prev = max(prev - 0.004, vec3(0.0));

    vec3 src = ringCol * ringGlow * (0.8 + uPulse * 0.7)
             + innerCol * innerGlow * (0.55 + uLoud * 0.55);

    vec3 outCol = max(prev, src);
    frag = vec4(min(outCol, vec3(1.2)), 0.0);
}"#;

const FRAG_SKYBOX: &str = r#"
vec3 sky_palette(float h, float sunset) {
    vec3 dayZenith    = vec3(0.12, 0.38, 0.78);
    vec3 dayHorizon   = vec3(1.00, 0.62, 0.32);
    vec3 nightZenith  = vec3(0.015, 0.02, 0.06);
    vec3 nightHorizon = vec3(0.14, 0.06, 0.22);
    vec3 zenith  = mix(dayZenith, nightZenith, sunset);
    vec3 horizon = mix(dayHorizon, nightHorizon, sunset);
    return mix(horizon, zenith, h);
}

float sky_cloudFbm(vec2 p, float t, float warp) {
    vec2 q = p + warp * vec2(fbm(p * 0.6 + t * 0.3), fbm(p * 0.6 - t * 0.2));
    return fbm(q + vec2(t, 0.0));
}

void main() {
    vec2 uv = vUv;
    vec2 p = aspect(vUv);

    // Slow day/night breathing cycle: continuous even in silence via uTime,
    // nudged along the beat grid via uPhase so the sky stays alive with the
    // music instead of drifting on a pure clock.
    float cyc = fract(uTime * 0.006 + uPhase * 0.015);
    float sunset = abs(cyc * 2.0 - 1.0);

    float horizonY = -0.06;

    // Sun sweeps the dome and sinks below the horizon as sunset advances;
    // uPulse kicks the disk radius + bloom strength on the beat.
    float sunX = sin(uTime * 0.025 + uPhase * 0.3) * 0.55;
    float sunY = mix(0.30, -0.55, sunset) + sin(uPhase * 6.28318) * 0.01;
    vec2 sunPos = vec2(sunX, sunY);
    vec2 sd = p - sunPos;
    float sunR = 0.045 + uPulse * 0.035;
    float sunDisk = smoothstep(sunR, sunR - 0.008, length(sd));
    float bloomFalloff = exp(-dot(sd, sd) * (55.0 - uPulse * 18.0));
    float bloomGate = smoothstep(0.32, 0.0, length(sd));
    float bloom = bloomFalloff * bloomGate * (0.45 + uPulse * 0.9);
    float sunVis = smoothstep(horizonY - 0.06, horizonY + 0.08, sunY);

    // Base sky gradient + horizon glow band; uLoud pumps the glow's
    // width and brightness.
    float h = clamp(uv.y, 0.0, 1.0);
    float skyH = smoothstep(horizonY, 0.55, p.y);
    vec3 sky = sky_palette(skyH, sunset);
    vec3 glowCol = mix(vec3(1.0, 0.55, 0.25), vec3(0.35, 0.15, 0.55), sunset);
    float hglow = smoothstep(0.16 + uLoud * 0.10, 0.0, abs(p.y - horizonY));
    sky += glowCol * hglow * (0.30 + uLoud * 0.65);
    float above = smoothstep(horizonY - 0.015, horizonY + 0.015, p.y);
    vec3 ground = mix(vec3(0.02, 0.02, 0.03), glowCol * 0.18, hglow);
    vec3 col = mix(ground, sky, above);

    // Layered fbm clouds, each layer scrolling at its own parallax speed;
    // uBass thickens the domain-warp turbulence and lowers the density
    // threshold so the deck gets thicker/stormier as bass rises. Narrow
    // smoothstep bands (was 0.38/0.40/0.42 wide -> blurred every layer into
    // a flat haze) so cloud edges read as defined shapes, not a soft wash.
    float turb = 0.20 + uBass * 1.1;
    vec2 cp = vec2(p.x * 1.3, p.y * 2.2);
    float c1 = sky_cloudFbm(cp * 1.1, uTime * 0.020, turb);
    float c2 = sky_cloudFbm(cp * 2.1 + 7.0, uTime * 0.045, turb * 0.8);
    float c3 = sky_cloudFbm(cp * 3.6 + 21.0, uTime * 0.085, turb * 0.6);
    float dens = 0.46 - uBass * 0.14;
    float cloud = smoothstep(dens, dens + 0.14, c1) * 0.55
                + smoothstep(dens + 0.04, dens + 0.16, c2) * 0.35
                + smoothstep(dens + 0.08, dens + 0.18, c3) * 0.22;
    float cloudMask = smoothstep(horizonY, horizonY + 0.06, p.y);
    cloud = clamp(cloud, 0.0, 1.0) * cloudMask;
    vec3 cloudCol = mix(vec3(1.0, 0.93, 0.85), vec3(0.45, 0.48, 0.62), sunset);
    col = mix(col, cloudCol, cloud * 0.85);

    // Sun disk + gated bloom: the exponential falloff is hard-clipped by
    // bloomGate so it never washes out the whole sky, only near the disk.
    vec3 sunTint = mix(vec3(1.0, 0.72, 0.32), vec3(0.65, 0.72, 1.0), sunset);
    col += bloom * sunTint * sunVis;
    col = max(col, vec3(sunDisk) * mix(vec3(1.0, 0.96, 0.85), vec3(0.85, 0.9, 1.0), sunset) * sunVis);

    // Sparse twinkling stars once night has settled; treble shimmers them.
    float starAmt = smoothstep(0.42, 0.78, sunset) * above;
    vec2 sp = floor((p + 0.5) * 90.0);
    float sh = hash(sp);
    float twinkle = 0.5 + 0.5 * sin(uTime * 3.0 + sh * 40.0 + uTreb * 6.0);
    float star = step(0.9965, sh) * twinkle * starAmt;
    col += vec3(star);

    vec3 finalCol;
    if (uRainbow == 1) {
        // Magic sky: hue-shift the whole composited scene, keeping its
        // brightness (structure of clouds/horizon/sun) intact as the
        // value channel so the layout still reads correctly.
        float lum = clamp(dot(col, vec3(0.299, 0.587, 0.114)), 0.0, 1.0);
        float hue = fract(h * 0.55 + cyc * 0.5 + uTime * 0.015);
        finalCol = hsv2rgb(vec3(hue, 0.62, lum * 1.1));
        finalCol = max(finalCol, vec3(sunDisk) * sunVis * 0.9);
        finalCol = max(finalCol, vec3(star));
    } else {
        // Accent-tint mode: was collapsing the whole composite to one flat
        // luminance value then applying a single flat uAccent multiply,
        // which crushed all the hue-carried structure (warm horizon vs
        // cool zenith, glow band, cloud deck, sun) into a hazy monochrome
        // wash. Instead scale uAccent per layer, same as every other mode
        // in this file, so the dome/horizon/cloud/sun bands stay distinct.
        vec3 skyTint = mix(uAccent * 0.85, uAccent * 0.22, skyH);
        skyTint += uAccent * hglow * (0.55 + uLoud * 0.9);
        vec3 groundTint = mix(uAccent * 0.05, uAccent * hglow * 0.4, hglow);
        finalCol = mix(groundTint, skyTint, above);
        finalCol = mix(finalCol, uAccent * (0.6 + cloud * 0.5), cloud * 0.85);
        finalCol += bloom * uAccent * 1.1 * sunVis;
        finalCol = max(finalCol, vec3(sunDisk) * uAccent * sunVis);
        finalCol += vec3(star) * uAccent;
    }

    frag = vec4(clamp(finalCol, 0.0, 1.6), 1.0);
}"#;

const FRAG_AURORA: &str = r#"
float aur_ridged(vec2 p) {
    float n = fbm(p);
    return 1.0 - abs(2.0 * n - 1.0);
}

void main() {
    vec2 p = aspect(vUv);

    // Cheap hash-cell starfield behind the curtains.
    vec2 starCell = floor(p * 70.0 + vec2(13.0, 7.0));
    float starH = hash(starCell);
    float starTw = 0.5 + 0.5 * sin(uTime * 2.2 + starH * 30.0 + uTreb * 4.0);
    float star = step(0.964, starH) * starTw;
    vec3 bg = vec3(0.008, 0.012, 0.026) + vec3(star) * 0.6;

    // Beat wave: a luminous band that travels bottom -> top of the sky on
    // the BPM sawtooth, gated per-curtain below.
    float waveY = mix(-0.68, 0.78, uPhase);

    // Curtains fade out near the top and bottom of frame.
    float vfade = smoothstep(-0.55, -0.10, p.y) * smoothstep(0.62, 0.12, p.y);

    vec3 col = bg;
    for (int i = 0; i < 3; i++) {
        float fi = float(i);
        float baseX = -0.55 + fi * 0.55;

        // Low-freq noise displaces the curtain spine horizontally; both the
        // sway amplitude and its speed grow with uMid.
        float swaySpeed = 0.05 + uMid * 0.12;
        float swayN = vnoise(vec2(p.y * 1.1 + fi * 9.3, uTime * swaySpeed + fi * 4.0));
        float spineX = baseX + (swayN - 0.5) * (0.28 + uMid * 0.55);
        float dx = p.x - spineX;

        // Ridged fbm along the curtain's length gives the folded, ribbon
        // look; its value also drives local width/brightness.
        vec2 ridgeCoord = vec2(fi * 4.0 + p.y * 2.2 - uTime * 0.12, spineX * 2.0 + fi * 2.0);
        float fold = aur_ridged(ridgeCoord);
        float width = 0.035 + fold * 0.10;
        float curtain = smoothstep(width, width * 0.15, abs(dx)) * (0.30 + fold * 0.9) * vfade;

        // Each beat sends a luminous wave running up this curtain.
        float waveHit = smoothstep(0.20, 0.0, abs(p.y - waveY)) * curtain;
        float emission = curtain * (0.28 + uLoud * 0.85) + waveHit * (0.8 + uPulse * 1.5);

        // Treble-driven sparkle shimmer, cell-hashed so it flickers instead
        // of crawling.
        vec2 sparkleCell = floor(vec2(dx * 70.0 + fi * 17.0, p.y * 45.0 + uTime * 6.0 + fi * 5.0));
        float sparkleH = hash(sparkleCell);
        float sparkle = step(0.978 - uTreb * 0.10, sparkleH) * curtain;
        emission += sparkle * (0.6 + uTreb * 1.3);

        vec3 bandCol;
        if (uRainbow == 1) {
            float hue = fract(0.30 + fi * 0.16 + p.y * 0.12 + uTime * 0.02 + uPhase * 0.06);
            bandCol = hsv2rgb(vec3(hue, 0.75, 1.0));
        } else {
            bandCol = uAccent;
        }
        col += bandCol * emission;
    }

    // Soft trailing feedback for flowing motion, biased down so it can
    // never pile up to blown white.
    vec3 prevCol = min(texture(uPrev, vUv).rgb, vec3(1.3));
    col = max(col, prevCol * 0.90 - 0.004);

    frag = vec4(clamp(col, 0.0, 1.6), 0.0);
}"#;

const FRAG_OUTRUN: &str = r#"
// gl_outrun - SYNTHWAVE SUNSET
// Banded sun with scanline gaps in its lower half, a mountain ridge whose
// skyline IS the live spectrum, and a BPM-locked perspective grid floor that
// flashes on every beat.

float mr_ridge(float x) {
    return spec(clamp(x, 0.0, 1.0));
}

void main() {
    vec2 p = aspect(vUv);

    // ---- sky gradient: deep purple up top, warm pink/orange at the horizon ----
    float skyT = clamp(p.y * 0.9 + 0.42, 0.0, 1.0);
    vec3 skyTop = (uRainbow == 1) ? hsv2rgb(vec3(0.74 + uTime * 0.008, 0.60, 0.22)) : uAccent * 0.14;
    vec3 skyBot = (uRainbow == 1) ? hsv2rgb(vec3(0.90 + uTime * 0.012, 0.55, 0.62)) : uAccent * 0.42;
    vec3 col = mix(skyBot, skyTop, skyT);

    // ---- giant banded sun, gaps widen toward the bottom half ----
    vec2 sunC = vec2(0.0, 0.10);
    float sunR = 0.30 + uLoud * 0.14 + uBass * 0.05;
    vec2 sp = p - sunC;
    float d = length(sp);
    float circleMask = smoothstep(sunR, sunR - 0.015, d);
    float local = sp.y / max(sunR, 0.0001);
    float lowerT = clamp(-local, 0.0, 1.0);
    float stripe = fract(local * 16.0 - uTime * 0.5);
    float duty = mix(1.0, 0.38, lowerT);
    float gapMask = step(1.0 - duty, stripe);
    float sunMask = circleMask * gapMask;

    float hueDrift = (uRainbow == 1) ? fract(uTime * 0.015) : 0.0;
    vec3 sunCol = (uRainbow == 1) ? hsv2rgb(vec3(0.01 + hueDrift * 0.12, 0.72, 1.0)) : uAccent;
    float bloom = exp(-d * d * (3.2 - uLoud * 1.6)) * (0.35 + uLoud * 1.1);
    col += sunCol * bloom * 0.55;
    col += sunCol * sunMask;

    // ---- mountain ridge silhouette: skyline height IS the spectrum ----
    float rx = clamp(p.x * 0.5 + 0.5, 0.0, 1.0);
    float ridgeH = -0.08 + mr_ridge(rx) * 0.42 + uMid * 0.03;
    float silhouette = smoothstep(0.006, -0.006, p.y - ridgeH);
    vec3 mtnCol = (uRainbow == 1) ? hsv2rgb(vec3(0.80 + uTime * 0.008, 0.65, 0.10)) : uAccent * 0.09;
    vec3 ridgeGlowCol = (uRainbow == 1) ? hsv2rgb(vec3(0.94, 0.65, 1.0)) : uAccent;
    float ridgeLine = smoothstep(0.02, 0.0, abs(p.y - ridgeH)) * (0.5 + uPulse * 1.3);
    col = mix(col, mtnCol, silhouette);
    col += ridgeGlowCol * ridgeLine * 0.55;

    // ---- perspective grid floor, scrolling toward the viewer, BPM-locked ----
    float floorY = -p.y;
    if (floorY > 0.0) {
        float bps = (uBpm > 0.0 ? uBpm : 120.0) / 60.0;
        float scroll = uTime * bps * 0.5;
        float persp = 0.5 / max(floorY, 0.02);
        vec2 gp = vec2(p.x * persp, persp - scroll);
        vec2 gf = abs(fract(gp) - 0.5);
        float cell = min(gf.x, gf.y);
        float lw = clamp(persp * 0.012, 0.0015, 0.05);
        float lineMask = smoothstep(lw, 0.0, cell);
        float fadeIn = smoothstep(0.0, 0.08, floorY);
        float flash = 0.55 + uPulse * 1.7 + uTreb * 0.2;

        vec3 floorBase = (uRainbow == 1)
            ? hsv2rgb(vec3(0.80 + uTime * 0.008, 0.55, 0.04 + floorY * 0.015))
            : uAccent * 0.035;
        vec3 lineCol = (uRainbow == 1)
            ? hsv2rgb(vec3(0.88 + gp.y * 0.004 + uTime * 0.01, 0.75, 1.0))
            : uAccent;
        vec3 floorCol = mix(floorBase, lineCol * flash, lineMask * fadeIn);

        float fog = exp(-floorY * 2.2);
        vec3 haze = (uRainbow == 1) ? hsv2rgb(vec3(0.92 + uTime * 0.01, 0.5, 0.6)) : uAccent * 0.5;
        floorCol = mix(floorCol, haze, fog * 0.55);
        col = floorCol;
    }

    col *= 1.0 + uPulse * 0.18;
    frag = vec4(col, 0.0);
}"#;

const FRAG_CITY: &str = r#"
// gl_city - SPECTRUM CITY
// Night city flythrough: a grid of box towers raymarched with a stepped
// heightfield (Terrain's trick, quantized into footprints so streets show
// through). Tower height comes from hist()/spec() buckets: x = street,
// depth = time, flying forward at bass speed. Camera stays well above the
// tallest possible tower (Terrain lesson: never spawn inside geometry).

float mr_twrHeight(vec2 cellId) {
    float bx = fract(cellId.x * 0.09 + 0.5);
    float bz = cellId.y * (1.0 / 24.0);
    float base = hist(vec2(bx, bz));            // 0..1 spectrum-history bucket
    float jitter = hash(cellId) * 0.15;          // per-tower variety
    float amp = 1.0 + uBass * 0.4 + uLoud * 0.25; // bounded 1.0..1.65
    return 0.10 + (base * 1.1 + jitter) * amp;    // bounded max ~2.16
}

void main() {
    vec2 p = aspect(vUv);

    // Camera: fixed high above the tallest possible tower (~2.16), gentle
    // stereo-correlation sway, looking down the street grid.
    vec3 ro = vec3(uCorr * 0.18, 2.7, 0.0);
    vec3 rd = normalize(vec3(p.x, p.y - 0.55, 1.0));

    // ---- sky / horizon glow (shown when a ray never dips into the grid) ----
    float glow = smoothstep(0.35, -0.02, rd.y) * (0.20 + uTreb * 0.35 + uFlux * 0.25);
    vec3 skyHue = (uRainbow == 1) ? hsv2rgb(vec3(0.62 + uTime * 0.01, 0.65, 1.0)) : uAccent;
    vec3 bgCol = skyHue * glow;
    bgCol += (uRainbow == 1) ? hsv2rgb(vec3(0.58, 0.4, 0.06)) : uAccent * 0.05;

    float cellSize = 0.55;
    float scroll = uTime * (0.9 + uBass * 2.4 + ((uBpm > 0.0) ? uBpm / 240.0 : 0.5));

    vec3 pos = ro;
    vec2 cellId = vec2(0.0);
    vec2 cellUV = vec2(0.0);
    float footHalf = 0.30;
    bool hit = false;
    float tt = 0.2;

    if (rd.y < -0.02) {
        for (int i = 0; i < 72; i++) {
            pos = ro + rd * tt;
            vec2 cellF = vec2(pos.x, pos.z + scroll) / cellSize;
            cellId = floor(cellF);
            cellUV = fract(cellF) - 0.5;
            footHalf = 0.28 + hash(cellId * 2.7) * 0.08;
            bool inFoot = max(abs(cellUV.x), abs(cellUV.y)) < footHalf;
            float h = inFoot ? mr_twrHeight(cellId) : 0.02;
            if (pos.y < h) { hit = true; break; }
            tt += 0.03 + tt * 0.05;
            if (tt > 11.0) break;
        }
    }

    vec3 col = bgCol;
    if (hit) {
        float twrHue = hash(cellId * 1.37 + 4.2);
        vec3 twrBase = (uRainbow == 1) ? hsv2rgb(vec3(twrHue, 0.5, 0.09)) : uAccent * 0.07;
        vec3 neonCol = (uRainbow == 1) ? hsv2rgb(vec3(fract(twrHue + 0.5), 0.85, 1.0)) : uAccent;

        // Lit windows: fract grid across the wall, thresholded by hash, flicker on treble.
        vec2 winGrid = vec2(cellUV.x * 6.0, pos.y * 5.0 + cellId.y * 1.7);
        vec2 winCell = floor(winGrid);
        float winOn = step(0.62, hash(winCell + cellId * 3.19));
        float flick = 0.65 + 0.35 * sin(uTime * 7.0 + hash(winCell + cellId) * 41.0);
        flick *= 0.55 + uTreb * 1.1 + uFlux * 0.7;
        float winLit = winOn * clamp(flick, 0.0, 1.6);

        // Neon rim near the footprint edge.
        float edgeDist = footHalf - max(abs(cellUV.x), abs(cellUV.y));
        float rim = smoothstep(0.055, 0.0, edgeDist);

        vec3 wallCol = twrBase + neonCol * winLit * 0.85 + neonCol * rim * (0.55 + uPulse * 1.3);
        float fog = exp(-tt * 0.15);
        col = mix(bgCol, wallCol, fog);
    }

    if (!hit) {
        float sGlow = spec(fract(p.x * 0.5 + 0.5 + uTime * 0.02)) * 0.10;
        col += ((uRainbow == 1) ? hsv2rgb(vec3(0.55, 0.6, 1.0)) : uAccent) * sGlow;
    }

    col *= 0.85 + uPulse * 0.5 + uLoud * 0.2;
    frag = vec4(col, 0.0);
}"#;

const FRAG_WORMHOLE: &str = r#"
// gl_wormhole — Milkdrop-style polar wormhole. The feedback buffer is
// swirl-warped and sucked toward the center; fresh light is injected each
// frame as a spectrum ring plus a stack of waveform ripples. The swirl
// direction eases into a flip once per beat bar.

float wh_hash1(vec2 p) { return hash(p); }

void main() {
    vec2 c = aspect(vUv);
    float r = length(c) + 1e-5;
    float a = atan(c.y, c.x);
    float ang01 = a / 6.28318 + 0.5;

    // Rotation direction eases to a flip once per beat bar (4 beats), driven
    // off uBpm's clock (falls back to a slow default tempo when unlocked).
    float bps = (uBpm > 1.0) ? (uBpm / 60.0) : 0.55;
    float barT = uTime * bps * 0.25;
    float barIdx = floor(barT);
    float barPh = fract(barT);
    float dirA = (mod(barIdx, 2.0) < 1.0) ? 1.0 : -1.0;
    float dirEase = smoothstep(0.80, 1.0, barPh);
    float dir = mix(dirA, -dirA, dirEase);

    // Swirl turbulence rides the spectral flux; suck speed rides bass.
    float turb = uFlux * 1.6;
    float swirl = dir * (0.55 + uBass * 1.6 + turb) / (r * 6.0 + 0.35);
    float ang = swirl * 0.02 + sin(uTime * 0.17 + r * 3.0) * turb * 0.012;

    // Pull toward center; bass and the beat pulse both deepen the suck.
    float zoom = 0.965 - uBass * 0.05 - uPulse * 0.02;

    float na = a + ang;
    vec2 w = vec2(cos(na), sin(na)) * (r * zoom);
    w.x /= uRes.x / uRes.y;
    vec3 prev = texture(uPrev, w + 0.5).rgb;

    // Feedback-heavy: decay under .96 plus a hard bias-down so trails can
    // never pile up to blown white.
    prev = prev * (0.958 - uFlux * 0.01) - 0.0035;
    prev = clamp(prev, 0.0, 1.15);

    // Fresh light: a spectrum ring sweeping with the beat phase.
    float ringR = 0.40 + spec(fract(ang01 + uPhase)) * 0.42;
    float ring = smoothstep(0.05, 0.0, abs(r - ringR))
                * (0.55 + uPulse * 1.1) * (0.6 + uLoud * 0.6);

    // Waveform ripples: three nested rings sampling the wave at staggered
    // angular offsets, breathing outward on the beat phase.
    float ripple = 0.0;
    for (int k = 0; k < 3; k++) {
        float fk = float(k);
        float wv = wav(fract(ang01 + fk * 0.13)) * 0.5;
        float rr2 = 0.14 + fk * 0.10 + wv * 0.22 + uPhase * 0.04;
        ripple += smoothstep(0.028, 0.0, abs(r - rr2)) * (1.0 - fk * 0.25);
    }

    // Treble sparkle injection: twinkling points scattered near the ring.
    float sector = floor(ang01 * 40.0);
    float bucket = floor(uTime * 6.0);
    float twinkle = wh_hash1(vec2(sector, bucket));
    float spark = step(0.965 - uTreb * 0.55, twinkle)
                * smoothstep(0.10, 0.0, abs(r - ringR - 0.03));

    vec3 base = (uRainbow == 1) ? hsv2rgb(vec3(ang01 + uTime * 0.04, 0.85, 1.0)) : uAccent;
    vec3 add = base * (ring * 0.95 + ripple * 0.55 + spark * 1.4);

    vec3 outCol = max(prev, add);
    outCol = min(outCol, vec3(1.3));
    frag = vec4(outCol, 1.0);
}"#;

const FRAG_SPIRO: &str = r#"
// gl_spiro — BPM-locked spirograph mandala. An epicyclic (hypotrochoid)
// curve with slowly morphing R/r/d parameters, stepping its rotation once
// per beat and easing across the beat via uPhase, rendered as glow from the
// minimum distance to ~60 sampled curve points. Feedback is rotated each
// frame for trails; a second mirrored curve spreads with stereo width.

vec2 spr_curve(float t, float R, float r, float d, float rot) {
    float tt = t + rot;
    float k = (R - r) / r;
    return vec2((R - r) * cos(tt) + d * cos(k * tt),
                (R - r) * sin(tt) - d * sin(k * tt));
}

void main() {
    vec2 p = aspect(vUv);

    // Rotation locked to the beat grid: one discrete step per beat, eased
    // across the beat by uPhase, plus a slow continuous drift so it never
    // fully freezes when unlocked/silent.
    float bps = (uBpm > 1.0) ? (uBpm / 60.0) : 0.6;
    float beatT = uTime * bps;
    float rot = (floor(beatT) + uPhase) * 0.5236 + uTime * 0.025;

    // Epicyclic params: slow independent morph, radii breathing on bass.
    float Rr = 0.30 + 0.07 * sin(uTime * 0.09) + uBass * 0.07;
    float rr = max(0.09 + 0.045 * sin(uTime * 0.13 + 1.7), 0.025);
    float dd = 0.15 + 0.09 * sin(uTime * 0.11 + 3.1) + uBass * 0.05;

    // A second mirrored curve spreads apart with stereo width.
    float spread = (uWidth - 1.0) * 0.4;

    const float N = 60.0;
    float mdist = 10.0;
    float bestHue = 0.0;
    float bestBr = 1.0;
    for (int i = 0; i < 60; i++) {
        float fi = float(i);
        float tI = fi / N * 6.28318;
        vec2 c1 = spr_curve(tI, Rr, rr, dd, rot);
        vec2 c2 = vec2(-c1.x - spread, c1.y);
        float br = spec(fract(fi / N + uPhase)) * 0.85 + 0.15;

        float d1 = length(p - c1);
        if (d1 < mdist) { mdist = d1; bestHue = fi / N; bestBr = br; }
        float d2 = length(p - c2);
        if (d2 < mdist) { mdist = d2; bestHue = fi / N; bestBr = br; }
    }

    float glow = smoothstep(0.05 + uTreb * 0.03, 0.0, mdist) * (0.55 + bestBr * 0.9);
    glow += smoothstep(0.012, 0.0, mdist) * uPulse * 1.4;
    glow = min(glow, 1.6);

    vec3 base = (uRainbow == 1) ? hsv2rgb(vec3(bestHue + uTime * 0.03, 0.85, 1.0)) : uAccent;
    vec3 mandala = base * glow;

    // Feedback trail: rotate the previous frame a touch each tick, kicking
    // harder on the beat pulse.
    float trailRot = 0.012 + uPulse * 0.05;
    float tca = cos(trailRot), tsa = sin(trailRot);
    vec2 w = mat2(tca, -tsa, tsa, tca) * p * (0.998 - uBass * 0.006);
    w.x /= uRes.x / uRes.y;
    vec3 prev = texture(uPrev, w + 0.5).rgb;
    prev = prev * (0.945 + uMid * 0.02) - 0.003;
    prev = clamp(prev, 0.0, 1.2);

    vec3 outCol = max(prev, mandala);
    outCol = min(outCol, vec3(1.3));
    frag = vec4(outCol, 1.0);
}"#;

const FRAG_LASER: &str = r#"
// gl_laser: dark club laser show. Three fan origins (bottom-left corner,
// bottom-right corner, top-center) each spray a fan of additive beams;
// BPM-locked sweep via uPhase, spread widens with uMid, strobe/multiply on
// uPulse + uFlux transients. fbm haze is lit by nearby beam brightness. A
// compressed, faded re-evaluation of the same fan near the bottom edge
// stands in for a floor reflection. uTreb adds one fast thin scanner beam
// per origin. Trails via uPrev, hard-biased down so they can never blow out.

vec3 lz_beams(vec2 p, float ar) {
    vec2 org0 = vec2(-0.46 * ar, -0.52);
    vec2 org1 = vec2(0.46 * ar, -0.52);
    vec2 org2 = vec2(0.0, 0.50);
    vec3 col = vec3(0.0);
    float fanSpread = 0.20 + uMid * 0.40;
    float strobe = 0.6 + uPulse * 1.7 + uFlux * 0.8;

    for (int oi = 0; oi < 3; oi++) {
        vec2 org = (oi == 0) ? org0 : (oi == 1) ? org1 : org2;
        float baseA = (oi == 0) ? (0.30 * 3.14159) : (oi == 1) ? (0.70 * 3.14159) : (-0.5 * 3.14159);
        float sweepDir = (oi == 0) ? 1.0 : (oi == 1) ? -1.0 : 1.3;
        float sweep = sin(uPhase * 6.28318 * sweepDir + float(oi) * 2.1) * fanSpread;
        float centerA = baseA + sweep;

        for (int j = 0; j < 4; j++) {
            float fj = float(j);
            float a = centerA + (fj / 3.0 - 0.5) * fanSpread * 1.6;
            vec2 dir = vec2(cos(a), sin(a));
            vec2 rel = p - org;
            float t = clamp(dot(rel, dir), 0.0, 1.7);
            vec2 closest = org + dir * t;
            float d = length(p - closest);
            float atten = exp(-t * 0.85);
            float core = exp(-d * d * 1100.0);
            float halo = exp(-d * 42.0) * 0.30;
            float e = spec(fract(fj / 4.0 + float(oi) * 0.33));
            float bright = (core + halo) * atten * strobe * (0.6 + e * 0.7);
            vec3 hue = (uRainbow == 1)
                ? hsv2rgb(vec3(fract(a / 6.28318 + float(oi) * 0.33 + uTime * 0.04), 0.85, 1.0))
                : uAccent;
            col += hue * bright;
        }

        // fast thin treble scanner beam sweeping independently
        float scanA = baseA + sin(uTime * (3.0 + uTreb * 9.0) + float(oi) * 1.7) * (fanSpread + 0.5);
        vec2 sdir = vec2(cos(scanA), sin(scanA));
        vec2 srel = p - org;
        float st = clamp(dot(srel, sdir), 0.0, 1.7);
        vec2 sclosest = org + sdir * st;
        float sd = length(p - sclosest);
        float sAtten = exp(-st * 0.7);
        float sCore = exp(-sd * sd * 5000.0);
        vec3 sHue = (uRainbow == 1)
            ? hsv2rgb(vec3(fract(scanA / 6.28318 + 0.5), 0.3, 1.0))
            : uAccent;
        col += sHue * sCore * sAtten * uTreb * 2.2;
    }
    return col;
}

void main() {
    float ar = uRes.x / uRes.y;
    vec2 p = aspect(vUv);

    vec3 col = lz_beams(p, ar);

    // faint floor reflection: compressed + faded re-evaluation near the bottom edge
    float floorY = -0.5;
    vec2 rp = vec2(p.x, floorY - (p.y - floorY) * 0.35 - 0.015);
    vec3 refl = lz_beams(rp, ar);
    float fade = 1.0 - smoothstep(-0.42, 0.05, p.y);
    col += refl * fade * 0.22;

    // volumetric haze: fbm fog lit by nearby beam brightness
    float fogN = fbm(vUv * 3.2 + vec2(uTime * 0.03, -uTime * 0.02));
    fogN += fbm(vUv * 6.5 - vec2(uTime * 0.05, uTime * 0.015)) * 0.5;
    vec3 hazeCol = (uRainbow == 1) ? hsv2rgb(vec3(0.58 + uTime * 0.015, 0.35, 1.0)) : uAccent;
    float lum = dot(col, vec3(0.299, 0.587, 0.114));
    col += hazeCol * fogN * (0.045 + lum * 0.5) * 0.5;

    // feedback trail: hard bias down so it can never pile to blown white
    vec3 prev = texture(uPrev, vUv).rgb * 0.90 - 0.006;
    prev = max(prev, vec3(0.0));
    col = max(col, prev);

    col = max(col, vec3(0.0));
    frag = vec4(col, 0.0);
}"#;

const FRAG_DISCOBALL: &str = r#"
// gl_discoball: mirror ball center-stage. fake-3D sphere shading, a
// rotating spherical facet grid whose cells glint when their hash aligns
// with a rotating light angle (uPulse-boosted), a ring of colored spotlight
// sweeps behind it driven by the full spectrum (spec buckets), and a
// rotating radial field of light spots thrown across the whole canvas at a
// uBpm-ish rate. uBass wobbles the ball's scale. Beat = brief camera-flash
// strobe. Dark background, feedback trails on the spots/ring only; the
// ball itself is solid and never smears. Trail decay hard-biased down.

void main() {
    vec2 p = aspect(vUv);

    float wobble = sin(uTime * 9.0) * uBass * 0.02;
    float R = 0.24 + uBass * 0.035 + wobble;
    float r = length(p);

    vec3 col = vec3(0.0);

    // rotating radial field of light spots thrown across the whole canvas
    float bpmSpeed = (uBpm > 1.0) ? clamp(uBpm / 120.0, 0.4, 3.0) : 1.0;
    vec3 spots = vec3(0.0);
    for (int i = 0; i < 14; i++) {
        float fi = float(i);
        float baseA = fi * (6.28318 / 14.0) + hash(vec2(fi, 3.1)) * 0.6;
        float a = baseA + uTime * 0.55 * bpmSpeed + sin(uPhase * 6.28318 + fi) * 0.10;
        float rad = 0.32 + hash(vec2(fi, 9.7)) * 0.5 + uBass * 0.05;
        vec2 sp = vec2(cos(a), sin(a)) * rad;
        float d = length(p - sp);
        float glow = exp(-d * d * 260.0) + exp(-d * 34.0) * 0.12;
        float e = spec(fract(fi / 14.0 + uPhase * 0.3));
        vec3 hue = (uRainbow == 1)
            ? hsv2rgb(vec3(fract(fi / 14.0 + uTime * 0.04), 0.85, 1.0))
            : uAccent;
        spots += hue * glow * (0.45 + e * 1.3) * (0.7 + uPulse * 0.8);
    }
    col += spots;

    // ring of colored spotlight sweeps behind the ball, driven by the spectrum
    float ringR = R + 0.14;
    float ringD = abs(r - ringR);
    float ringMask = smoothstep(0.06, 0.0, ringD);
    float ang = atan(p.y, p.x) / 6.28318 + 0.5;
    float sweepE = spec(fract(ang + uTime * 0.09));
    vec3 ringHue = (uRainbow == 1) ? hsv2rgb(vec3(ang + uTime * 0.05, 0.9, 1.0)) : uAccent;
    col += ringHue * ringMask * (0.12 + sweepE * 1.2);

    // beat = brief camera-flash strobe
    float flash = smoothstep(0.7, 1.0, uPulse);
    col += vec3(1.0) * flash * 0.4;

    // feedback trails on the spots/ring only, hard-biased down
    vec3 prev = texture(uPrev, vUv).rgb * 0.92 - 0.008;
    prev = max(prev, vec3(0.0));
    col = max(col, prev);

    // the mirror ball itself: solid facet grid, no feedback smear
    if (r < R) {
        vec2 nxy = p / R;
        float nz = sqrt(max(0.0, 1.0 - dot(nxy, nxy)));
        vec3 nrm = vec3(nxy, nz);

        float ballRot = uTime * 0.35;
        float theta = atan(nrm.y, nrm.x) + ballRot;
        float phi = acos(clamp(nrm.z, -1.0, 1.0));
        float cu = theta / 6.28318 * 22.0;
        float cv = phi / 3.14159 * 11.0;
        vec2 cellId = floor(vec2(cu, cv));
        vec2 cellUv = fract(vec2(cu, cv));

        float ch = hash(cellId);
        float lightAngle = fract(uTime * 0.15 + uPulse * 0.04);
        float da = abs(fract(ch - lightAngle + 0.5) - 0.5);
        float glint = smoothstep(0.05, 0.0, da);

        vec3 lightDir = normalize(vec3(0.35, 0.55, 0.75));
        float ndl = clamp(dot(nrm, lightDir), 0.0, 1.0);

        vec2 fc = cellUv - 0.5;
        float facetEdge = 1.0 - smoothstep(0.30, 0.48, max(abs(fc.x), abs(fc.y)));

        vec3 facetBase = (uRainbow == 1)
            ? hsv2rgb(vec3(ch, 0.5, 0.30 + ndl * 0.45))
            : uAccent * (0.25 + ndl * 0.4);
        vec3 glintCol = mix(facetBase, vec3(1.0), 0.75);

        vec3 ballCol = mix(facetBase * 0.45, facetBase, ndl) * facetEdge;
        ballCol += glintCol * glint * (0.6 + uPulse * 1.5 + flash) * facetEdge;
        ballCol += vec3(0.03, 0.04, 0.06) * (1.0 - facetEdge);

        float rim = pow(1.0 - nz, 2.5) * 0.3;
        ballCol += vec3(0.5, 0.6, 0.85) * rim * ndl;

        float edgeMask = smoothstep(0.0, 0.012, R - r);
        col = mix(col, ballCol + flash * 0.15, edgeMask);
    }

    frag = vec4(col, 0.0);
}"#;

const FRAG_HEXGRID: &str = r#"
// gl_hexgrid: proper axial hex-tiling grid lit by the spectrum, with a
// beat-locked radial ripple skewed into an ellipse by stereo correlation.
// Pointy-top axial coordinates (redblobgames convention) + cube rounding
// for exact hex assignment, then an exact hex-boundary norm for the edges.

vec2 mr_hexAxial(vec2 p, float size) {
    float q = (0.5773502692 * p.x - 0.3333333333 * p.y) / size;
    float r = (0.6666666667 * p.y) / size;
    return vec2(q, r);
}

vec2 mr_hexCenter(vec2 ax, float size) {
    float x = size * (1.7320508076 * ax.x + 0.8660254038 * ax.y);
    float y = size * (1.5 * ax.y);
    return vec2(x, y);
}

vec3 mr_cubeRound(vec3 c) {
    vec3 rc = floor(c + 0.5);
    vec3 d = abs(rc - c);
    if (d.x > d.y && d.x > d.z) {
        rc.x = -rc.y - rc.z;
    } else if (d.y > d.z) {
        rc.y = -rc.x - rc.z;
    } else {
        rc.z = -rc.x - rc.y;
    }
    return rc;
}

vec2 mr_hexRound(vec2 ax) {
    vec3 cube = vec3(ax.x, -ax.x - ax.y, ax.y);
    cube = mr_cubeRound(cube);
    return vec2(cube.x, cube.z);
}

// Hexagonal norm: exactly 1.0 (in units of the apothem) on the true hex
// boundary, for any pointy-top hex centered at the origin. Cheap stand-in
// for a full SDF, plenty for a soft neon edge.
float mr_hexDist(vec2 p) {
    vec2 q = abs(p);
    return max(q.x, 0.5 * q.x + 0.8660254038 * q.y);
}

void main() {
    vec2 p = aspect(vUv);

    // Slow whole-grid rotation for life; cube-rounding stays exact under it.
    float ang = uTime * 0.02;
    float ca = cos(ang), sa = sin(ang);
    vec2 rp = mat2(ca, -sa, sa, ca) * p;

    float size = 0.088 + uBass * 0.006;
    vec2 ax = mr_hexAxial(rp, size);
    vec2 cell = mr_hexRound(ax);
    vec2 center = mr_hexCenter(cell, size);
    vec2 local = rp - center;
    float apothem = size * 0.8660254038;
    float hn = mr_hexDist(local) / apothem;

    // Spectrum bucket per cell: a smooth diagonal sweep across the axial
    // grid, so the hex field reads as a frequency landscape.
    float bucket = fract(cell.x * 0.055 + cell.y * 0.03 + 0.5);
    float e = spec(bucket);

    // Radial ripple(s) locked to the beat grid (uPhase), skewed into an
    // ellipse by stereo correlation: uCorr +1 (mono/narrow) tightens it on
    // x, -1 (wide/out-of-phase) tightens it on y: the gonio shape drives
    // the ripple geometry directly.
    vec2 ep = vec2(p.x * (1.0 - uCorr * 0.32), p.y * (1.0 + uCorr * 0.32));
    float r = length(ep);
    float ripple = 0.0;
    for (int k = 0; k < 3; k++) {
        float ph = fract(uPhase + float(k) / 3.0);
        float ringR = ph * 1.15;
        ripple += smoothstep(0.10, 0.0, abs(r - ringR)) * (1.0 - ph) * (0.5 + uPulse * 0.7);
    }

    float glowFloor = uLoud * 0.16;
    float fillMask = 1.0 - smoothstep(0.55, 1.02, hn);
    float fill = fillMask * (0.10 + e * 0.55 + ripple * 0.9 + glowFloor);
    float edgeMask = 1.0 - smoothstep(0.0, 0.05, abs(hn - 0.94));
    float twinkle = 0.85 + 0.15 * sin(uTime * 8.0 + hash(cell) * 6.283185);
    float edge = edgeMask * (0.6 + e * 0.9 + ripple * 1.3 + uTreb * 0.3) * mix(1.0, twinkle, uTreb);

    float hue = fract(cell.x * 0.10 + cell.y * 0.065 + uTime * 0.02);
    vec3 cellCol = (uRainbow == 1) ? hsv2rgb(vec3(hue, 0.78, 1.0)) : uAccent;
    vec3 newCol = cellCol * (fill * 0.6 + edge);

    // Soft neon bloom: cheap 4-tap blur of the feedback buffer, biased down
    // hard (multiply + floor subtract) so it can never pile toward blown
    // white, only ever accumulate a diffuse glow around bright edges.
    vec2 texel = 1.0 / uRes;
    vec3 blur = vec3(0.0);
    blur += texture(uPrev, vUv + vec2( texel.x * 1.6, 0.0)).rgb;
    blur += texture(uPrev, vUv + vec2(-texel.x * 1.6, 0.0)).rgb;
    blur += texture(uPrev, vUv + vec2(0.0,  texel.y * 1.6)).rgb;
    blur += texture(uPrev, vUv + vec2(0.0, -texel.y * 1.6)).rgb;
    blur *= 0.25 * (0.90 + uMid * 0.04);
    blur = max(blur - 0.006, 0.0);

    frag = vec4(min(max(blur, newCol), vec3(1.3)), 0.0);
}"#;

const FRAG_LIGHTNING: &str = r#"
// gl_lightning: electric storm, fbm-displaced bolts gated by the beat,
// thinner branch hints, afterglow via feedback, sky flash over drifting
// fbm clouds. Bolt anchors track the loudest spectrum bucket in their own
// frequency lane so the storm visibly listens to the mix.

// Cheapest-possible "loudest bucket in [lo,hi]" scan: 8 taps, bounded loop.
vec2 mr_loudBucket(float lo, float hi) {
    float bestV = -1.0;
    float bestX = (lo + hi) * 0.5;
    for (int i = 0; i < 8; i++) {
        float x = mix(lo, hi, (float(i) + 0.5) / 8.0);
        float v = spec(x);
        if (v > bestV) {
            bestV = v;
            bestX = x;
        }
    }
    return vec2(bestX, bestV);
}

// Vertical fbm-displaced bolt path: x offset as a function of screen-space
// height y (0 = ground, 1 = sky). Zigzag widens on approach to the ground.
float mr_boltX(float y, float anchorX, float seed) {
    float n1 = fbm(vec2(seed, y * 5.0 + uTime * 0.4)) - 0.5;
    float n2 = fbm(vec2(seed + 9.1, y * 11.0 - uTime * 0.7)) - 0.5;
    float wobble = n1 * 0.22 + n2 * 0.08;
    wobble *= 0.35 + (1.0 - y) * 0.9;
    return anchorX + wobble;
}

void main() {
    vec2 p = aspect(vUv);
    float ar = uRes.x / uRes.y;
    float ys = clamp(vUv.y, 0.0, 1.0);

    // Sky: drifting fbm clouds, always alive via uTime alone, sheet-lit by
    // the beat pulse and momentary loudness on top.
    float cloud = fbm(p * 1.7 + vec2(uTime * 0.025, uTime * 0.012));
    float flash = uPulse * 0.85 + uLoud * 0.22;
    vec3 sky = mix(vec3(0.015, 0.02, 0.035), vec3(0.14, 0.17, 0.24), cloud) * (0.35 + flash);

    // Stereo correlation widens/narrows the whole storm's footprint; onset
    // flux (transients) gates how much branch bolts show through.
    float spread = 1.0 + (1.0 - uCorr) * 0.22;
    float branchGate = smoothstep(0.08, 0.55, uFlux);

    vec3 accum = vec3(0.0);
    for (int i = 0; i < 4; i++) {
        float fi = float(i);
        // Higher-indexed bolts need a bigger uPulse to wake up: quiet beats
        // show 2 bolts, big hits bring in all 4.
        float thresh = 0.10 + fi * 0.17;
        float exist = smoothstep(thresh - 0.14, thresh + 0.06, uPulse);
        float ambient = 0.05 + 0.025 * sin(uTime * 0.6 + fi * 2.1);

        float lo = fi * 0.25;
        vec2 lb = mr_loudBucket(lo, lo + 0.25);
        float slot = (fi - 1.5) / 3.0 * ar * 0.8 * spread;
        float nudge = (lb.x - (lo + 0.125)) * 1.8;
        float anchor = slot + nudge + sin(uTime * 0.11 + fi * 3.3) * 0.04;

        float bx = mr_boltX(ys, anchor, fi * 13.7 + 1.0);
        float d = p.x - bx;
        float d2 = d * d;
        float width = 0.00035 + lb.y * 0.00045;
        float core = (exist * (0.7 + uPulse * 0.6) + ambient) * width / (d2 + 0.00012);

        float coreWhite = 1.0 - smoothstep(0.0, 0.0025, d2);
        vec3 hueCol = (uRainbow == 1)
            ? hsv2rgb(vec3(fract(fi / 4.0 + uTime * 0.05), 0.65, 1.0))
            : uAccent;
        vec3 boltCol = mix(hueCol, vec3(1.0), coreWhite);

        accum += boltCol * core;

        // Thinner branch bolt hint: diverges from the main path in the
        // lower part of the frame only, dimmer and thinner.
        float branchStart = 0.55 + hash(vec2(fi, 4.0)) * 0.15;
        float branchDiv = (fbm(vec2(fi * 3.0 + 9.0, ys * 4.0 - uTime * 0.5)) - 0.5) * 0.16;
        float bxx = mr_boltX(ys, anchor + branchDiv, fi * 13.7 + 22.0);
        float bd = p.x - bxx;
        float bd2 = bd * bd;
        float bmask = (1.0 - smoothstep(branchStart - 0.12, branchStart, ys)) * branchGate;
        float bcore = (exist * 0.5 + ambient * 0.5) * 0.00014 / (bd2 + 0.00011) * bmask;

        accum += boltCol * bcore * 0.8;
    }

    // Afterglow: biased-down feedback so a strike lingers briefly and fades,
    // never piling toward blown white.
    vec3 prev = texture(uPrev, vUv).rgb * 0.90;
    prev = max(prev - 0.01, 0.0);
    vec3 col = sky * 0.55 + accum;
    col = min(col, vec3(1.4));
    frag = vec4(max(prev, col), 0.0);
}"#;

const FRAG_DNA: &str = r#"
// gl_dna — rotating double helix down the screen center. Two sinusoid
// strands (phase offset PI) fake depth via cos()->scale+brightness so the
// front/back strand swaps as it spins. Rungs connect the strands, lit by the
// spectrum bucket at their height. Twist speed rides uBpm (confidence-blended
// with a calm fallback so it still moves when unlocked), radius breathes on
// uBass, a bright pulse travels the length once per beat on uPhase/uPulse,
// and light feedback trails persist the motion.

float dna_theta(float y, float ph, float spin, float twist) {
    return y * twist + spin + ph;
}

// x = strand's horizontal offset at this y; y (of the return) = raw cos
// depth term (-1 back .. 1 front) used for the front/back fake-depth trick.
vec2 dna_strand(float y, float ph, float spin, float twist, float radius) {
    float th = dna_theta(y, ph, spin, twist);
    return vec2(sin(th) * radius, cos(th));
}

void main() {
    vec2 p = aspect(vUv);
    float y = p.y;
    float t = uTime;

    float bpmNorm = (uBpm > 1.0) ? uBpm / 120.0 : 1.0;
    float spinSpeed = mix(0.55, bpmNorm, clamp(uBpmConf, 0.0, 1.0));
    float spin = t * (0.7 + spinSpeed * 0.9) + uPulse * 0.15;
    float twist = 18.0 + uMid * 7.0;
    float radius = 0.14 + uBass * 0.11 + uPulse * 0.025 + (uWidth - 1.0) * 0.02;
    radius = clamp(radius, 0.05, 0.32);

    vec2 sA = dna_strand(y, 0.0, spin, twist, radius);
    vec2 sB = dna_strand(y, 3.14159265, spin, twist, radius);
    float depthA = sA.y * 0.5 + 0.5;
    float depthB = sB.y * 0.5 + 0.5;
    float xA = sA.x;
    float xB = sB.x;

    float thickA = mix(0.0035, 0.011, depthA);
    float thickB = mix(0.0035, 0.011, depthB);
    float brightA = mix(0.30, 1.0, depthA);
    float brightB = mix(0.30, 1.0, depthB);

    float dA = abs(p.x - xA);
    float dB = abs(p.x - xB);
    float lineA = smoothstep(thickA, 0.0, dA) * brightA;
    float lineB = smoothstep(thickB, 0.0, dB) * brightB;
    float haloA = smoothstep(thickA * 5.0, 0.0, dA) * brightA * 0.18;
    float haloB = smoothstep(thickB * 5.0, 0.0, dB) * brightB * 0.18;

    // Rungs: connect the strands at quantized y-steps, brightness pulled from
    // the spectrum bucket that height maps to.
    float rungFreq = 15.0;
    float ry = floor(y * rungFreq + 0.5) / rungFreq;
    vec2 rA = dna_strand(ry, 0.0, spin, twist, radius);
    vec2 rB = dna_strand(ry, 3.14159265, spin, twist, radius);
    float rxA = rA.x;
    float rxB = rB.x;
    float lo = min(rxA, rxB);
    float hi = max(rxA, rxB);
    float insideH = smoothstep(0.0, 0.012, p.x - lo) * smoothstep(0.0, 0.012, hi - p.x);
    float insideV = smoothstep(0.009, 0.0, abs(y - ry));
    float bucket = clamp(ry + 0.5, 0.0, 1.0);
    float rungE = spec(bucket);
    float rung = insideH * insideV * (0.25 + rungE * 1.1);

    // A bright pulse travels the full length of the helix once per beat.
    float travel = fract(uPhase);
    float bandY = mix(-0.55, 0.55, travel);
    float band = smoothstep(0.10, 0.0, abs(y - bandY)) * uPulse;

    float fluxSpike = smoothstep(0.35, 1.0, uFlux);

    vec3 col;
    if (uRainbow == 1) {
        float hueA = fract(y * 0.7 - t * 0.06 + depthA * 0.12);
        float hueB = fract(y * 0.7 - t * 0.06 + 0.5 + depthB * 0.12);
        float hueR = fract(ry * 0.7 - t * 0.06 + 0.25);
        vec3 colA = hsv2rgb(vec3(hueA, 0.75, 1.0));
        vec3 colB = hsv2rgb(vec3(hueB, 0.75, 1.0));
        vec3 colR = hsv2rgb(vec3(hueR, 0.55, 1.0));
        col = colA * (lineA + haloA) + colB * (lineB + haloB) + colR * rung;
    } else {
        col = uAccent * (lineA + haloA + lineB + haloB) + uAccent * rung * 1.1;
    }

    col += vec3(1.0) * band * (lineA + lineB + rung) * 1.3;
    col += vec3(1.0, 0.95, 0.85) * fluxSpike * (lineA + lineB) * 0.35;
    col *= (0.85 + uLoud * 0.35);

    // Light feedback trail: biased well under 1.0 plus a floor so it can
    // never pile up toward white, only ghost the strands' recent motion.
    vec3 prev = texture(uPrev, vUv).rgb * (0.90 - uTreb * 0.05);
    prev = max(prev - 0.0035, 0.0);

    frag = vec4(max(col, prev), 0.0);
}"#;

const FRAG_BUBBLES: &str = r#"
// gl_bubbles — rising bubbles over an underwater gradient with fbm caustic
// shimmer. ~30 hash-seeded bubbles, each locked to one spectrum bucket for
// its radius, wobbling on vnoise as it rises. Rim + specular dot + faint
// fill per bubble, rainbow hue per bubble. Beat = every bubble squash-jumps;
// flux spikes give individual bubbles a pop-flash. Fully self-contained per
// frame (no uPrev feedback) so the opaque backdrop can never accumulate.

vec3 bub_render(vec2 p, vec2 center, float rb, float squash, float stretch, float hue) {
    vec2 d = p - center;
    d.x /= stretch;
    d.y /= squash;
    float dist = length(d);

    float ringThick = 0.006 + rb * 0.05;
    float rim = smoothstep(ringThick, 0.0, abs(dist - rb));
    float fill = (1.0 - smoothstep(0.0, rb, dist)) * 0.14;

    vec2 specPos = center + vec2(-0.32, 0.30) * rb;
    float specDist = length(p - specPos);
    float specDot = smoothstep(rb * 0.30, 0.0, specDist);

    vec3 bcol = (uRainbow == 1) ? hsv2rgb(vec3(hue, 0.65, 0.95)) : uAccent;
    return bcol * (rim * 1.1 + fill) + vec3(1.0) * specDot * 0.85;
}

void main() {
    vec2 p = aspect(vUv);
    float t = uTime;

    // Underwater backdrop: deep at the bottom, lit toward the surface.
    float causticMix = (uRainbow == 1) ? 1.0 : 0.55;
    vec3 deep = (uRainbow == 1) ? hsv2rgb(vec3(0.58, 0.78, 0.09)) : uAccent * 0.05;
    vec3 shallow = (uRainbow == 1) ? hsv2rgb(vec3(0.50, 0.55, 0.40)) : uAccent * 0.26;
    vec3 col = mix(deep, shallow, pow(clamp(vUv.y, 0.0, 1.0), 1.3));

    vec2 causticUv = p * 3.1 + vec2(t * 0.05, -t * 0.11);
    float caustic = fbm(causticUv + fbm(causticUv * 1.7 + t * 0.2) * 0.6);
    caustic = pow(clamp(caustic, 0.0, 1.0), 2.0);
    col += vec3(0.55, 0.85, 0.9) * caustic * (0.10 + uMid * 0.14) * causticMix;

    for (int i = 0; i < 30; i++) {
        float fi = float(i);
        float colSeed = hash(vec2(fi, 1.0));
        float riseH    = hash(vec2(fi, 5.0));
        float phaseH   = hash(vec2(fi, 9.0));
        float sizeH    = hash(vec2(fi, 13.0));
        float hueH     = hash(vec2(fi, 17.0));
        float popH     = hash(vec2(fi, 21.0));
        float wobH     = hash(vec2(fi, 25.0));
        float bucket = fi / 30.0;

        // Rise: per-bubble speed from uBass plus per-bubble variety, looping
        // seamlessly from well below to well above the visible frame.
        float speed = 0.05 + uBass * 0.4 + riseH * 0.14;
        float life = fract(t * speed + phaseH * 12.0);
        float by = mix(-0.62, 0.62, life);

        float wob = (vnoise(vec2(fi * 3.3 + wobH * 5.0, t * 0.5 + fi)) - 0.5)
                  * (0.05 + 0.05 * sin(t * 0.7 + wobH * 6.28318));
        float bx = (colSeed - 0.5) * 1.7 + wob;

        // Radius from this bubble's assigned spectrum bucket.
        float rb = 0.018 + spec(clamp(bucket, 0.0, 1.0)) * 0.075;
        rb *= 0.65 + sizeH * 0.7;

        // Pop-flash on spectral-flux spikes, staggered per bubble so a spike
        // doesn't flash the whole swarm identically.
        float popBoost = smoothstep(0.4, 1.0, uFlux) * (0.4 + 0.6 * popH);
        rb *= 1.0 + popBoost * 0.25;

        // Beat = every bubble squashes flat and hops.
        float squash = 1.0 - uPulse * 0.32;
        float stretch = 1.0 + uPulse * 0.32;
        by += uPulse * 0.03;

        // Fade across the respawn seam (already off-screen, but keeps the
        // edges soft for taller canvases).
        float edgeFade = smoothstep(0.0, 0.08, life) * (1.0 - smoothstep(0.92, 1.0, life));

        float hue = fract(hueH + t * 0.02);
        vec3 add = bub_render(p, vec2(bx, by), rb, squash, stretch, hue);
        col += add * (1.0 + popBoost * 2.0) * edgeFade;
    }

    frag = vec4(col, 0.0);
}"#;

const FRAG_COPPER: &str = r#"
// gl_copper — Amiga demoscene copper bars. Ten bright horizontal raster
// bars, each bouncing on its own sine phase locked to the BPM grid, bar
// thickness pumping on bass, additive crossings glowing hotter, per-bar
// hue cycling (rainbow) or accent shading, treble scanline shimmer.

float cop_falloff(float d, float w) {
    // Bright core fading softly both ways (up AND down) from the bar center.
    float core = exp(-(d * d) / (w * w * 0.35));
    float halo = exp(-(d * d) / (w * w * 3.2)) * 0.35;
    return core + halo;
}

vec3 cop_barColor(float idx, float n, float energy) {
    if (uRainbow == 1) {
        float hue = fract(idx / n + uTime * 0.045 + uPulse * 0.08);
        return hsv2rgb(vec3(hue, 0.82, 1.0));
    }
    float shade = 0.55 + 0.45 * sin(idx * 1.7 + uTime * 0.35);
    return uAccent * (0.6 + shade * 0.6) + uAccent * energy * 0.15;
}

void main() {
    vec2 uv = vUv;

    // Trail feedback biased hard down: never lets the raster pile to white.
    vec3 trail = texture(uPrev, uv).rgb * 0.90 - 0.006;
    trail = max(trail, vec3(0.0));

    float thickness = 0.020 + uBass * 0.045 + uPulse * 0.01;
    float nBars = 10.0;
    vec3 col = vec3(0.0);

    for (int i = 0; i < 10; i++) {
        float fi = float(i);

        // Per-bar unique bounce speed via a golden-ratio spread, driven off
        // the BPM-locked phase grid so every bar stays in the pocket even as
        // it drifts out of sync with its neighbours.
        float speedMul = 0.55 + fract(fi * 0.6180339) * 1.15;
        float basePhase = uPhase * 6.28318 * speedMul + fi * 2.39996;
        float drift = uTime * (0.10 + speedMul * 0.06);
        float bounce = sin(basePhase + drift) * 0.5
                      + sin(basePhase * 1.9 + drift * 1.3 + fi) * 0.14;
        float centerY = 0.5 + bounce * 0.42;

        float d = uv.y - centerY;
        float bar = cop_falloff(d, thickness);

        float energy = spec(fi / nBars);
        bar *= 0.5 + energy * 1.3 + uPulse * 0.55;

        col += cop_barColor(fi, nBars, energy) * bar;
    }

    // Additive crossings read hotter automatically from the sum above; push
    // the brightest overlaps a touch further for that copper-bar glare.
    col += col * col * 0.12;

    // Treble-driven scanline shimmer, classic raster interference look.
    float scan = sin(uv.y * uRes.y * 1.35 + uTime * 24.0) * 0.5 + 0.5;
    col *= 1.0 + scan * uTreb * 0.30;
    float interlace = fract(uv.y * uRes.y * 0.5);
    col *= 0.92 + 0.08 * interlace;

    col = max(col, trail);
    col = min(col, vec3(1.25));

    frag = vec4(col, 1.0);
}"#;

const FRAG_LEDWALL: &str = r#"
// gl_ledwall — stadium LED matrix wall. 64x36 coarse cell grid (64 columns
// lines up 1:1 with the spectrum texture's bin count), each cell a rounded
// RGB-subpixel LED dot against a visible off-state grid, columns driven by
// the spectrum as a green-yellow-red (or rainbow) ladder with slow-falling
// peak-hold caps, mild barrel bulge, uPulse full-wall flash, uLoud floor glow.

const float LED_COLS = 64.0;
const float LED_ROWS = 36.0;

vec3 led_ladder(float t) {
    // t: 0 (bottom, quiet) -> 1 (top, loud). Green -> yellow -> red.
    float hue = mix(0.34, 0.0, clamp(t, 0.0, 1.0));
    return hsv2rgb(vec3(hue, 0.92, 1.0));
}

float led_dot(vec2 cellF, vec2 off) {
    float r = length(cellF - off);
    // Descending edges: 1.0 at the dot core, fading to 0.0 by the cell gap.
    return smoothstep(0.40, 0.16, r);
}

void main() {
    // Mild barrel bulge: the "shot off a stadium screen" lens vibe.
    vec2 c = vUv - 0.5;
    float r2 = dot(c, c);
    vec2 uvB = clamp(c * (1.0 + 0.10 * r2) + 0.5, 0.0, 1.0);

    vec2 cellPos = uvB * vec2(LED_COLS, LED_ROWS);
    vec2 cellId = floor(cellPos);
    vec2 cellF = fract(cellPos) - 0.5;

    float colNorm = (cellId.x + 0.5) / LED_COLS;
    float rowNorm = (cellId.y + 0.5) / LED_ROWS;

    float energy = spec(colNorm);
    float barHeight = clamp(energy * (0.92 + uBass * 0.18), 0.0, 1.0);

    // Peak-hold cap: every pixel in a column writes the SAME value into the
    // alpha channel, so sampling one fixed row of last frame reliably gives
    // that column's held peak back, wherever on the wall we currently are.
    vec2 peakUV = vec2(colNorm, 0.5);
    float prevPeak = texture(uPrev, peakUV).a;
    float peak = clamp(max(barHeight, prevPeak - 0.0028), 0.0, 1.0);

    bool lit = rowNorm <= barHeight;
    bool isCap = abs(rowNorm - peak) < (0.65 / LED_ROWS) && peak > barHeight + 0.006;

    // RGB sub-pixel offsets inside each cell: real LED walls show this as a
    // faint moire when the grid is coarse and shot slightly off-axis.
    float dR = led_dot(cellF, vec2(-0.085, 0.0));
    float dG = led_dot(cellF, vec2(0.0, 0.0));
    float dB = led_dot(cellF, vec2(0.085, 0.0));

    vec3 ladder = (uRainbow == 1)
        ? led_ladder(rowNorm)
        : uAccent * (0.55 + rowNorm * 0.85);

    float floorGlow = 0.02 + uLoud * 0.10;
    vec3 offCol = vec3(0.02, 0.022, 0.025) + floorGlow * ladder * 0.3;
    vec3 onCol = ladder * (0.75 + energy * 0.9);
    vec3 capCol = mix(ladder, vec3(1.0), 0.65) * 1.4;

    vec3 pix = lit ? onCol : offCol;
    pix = isCap ? capCol : pix;

    // BPM-locked chase highlight sweeping across the columns on the beat.
    float chase = smoothstep(0.05, 0.0, abs(fract(uPhase) - colNorm));
    vec3 chaseCol = (uRainbow == 1) ? hsv2rgb(vec3(colNorm, 0.85, 1.0)) : uAccent;
    pix += chaseCol * chase * 0.30 * (0.3 + uMid * 0.7);

    // Composite the three sub-pixel channels through their own dot masks —
    // this is what sells the moire/subpixel read at normal viewing size.
    vec3 col = vec3(pix.r * dR, pix.g * dG, pix.b * dB);

    // Full-wall brightness lift on the beat.
    col *= 1.0 + uPulse * 0.55;
    col += floorGlow * 0.05;

    frag = vec4(clamp(col, 0.0, 1.4), peak);
}"#;

const FRAG_SONAR: &str = r#"
// gl_sonar — CRT radar/sonar scope. Dark phosphor disc with range rings and
// bearing spokes, a rotating sweep beam locked to the BPM grid (one
// revolution per bar), spectrum blips (angle = bucket, radius = energy),
// a beat ping expanding from the core, and a decaying phosphor wake via
// feedback. Scanlines + CRT vignette finish it off.

float sn_ring(float r, float radius, float width) {
    return smoothstep(width, 0.0, abs(r - radius));
}

float sn_wrap(float a) {
    return mod(a + 3.14159265, 6.28318531) - 3.14159265;
}

void main() {
    vec2 c = aspect(vUv);
    float r = length(c);
    float ang = atan(c.y, c.x);
    float scopeR = 0.46;

    // One sweep revolution per bar (4 beats) when the deck is BPM-locked;
    // a slow idle spin otherwise so the scope keeps breathing in silence.
    float revPerSec = (uBpm > 1.0) ? (uBpm / 240.0) : 0.05;
    float sweep = sn_wrap(uTime * revPerSec * 6.28318531);

    // Phosphor decay wake: hard bias-down + floor so trails can never pile
    // up to blown white.
    vec3 prev = texture(uPrev, vUv).rgb;
    prev = max(prev * 0.955 - 0.005, 0.0);

    vec3 phosphor = (uRainbow == 1)
        ? hsv2rgb(vec3(0.32 + 0.04 * sin(uTime * 0.07), 0.9, 1.0))
        : uAccent;

    vec3 col = vec3(0.0);

    if (r < scopeR) {
        col += phosphor * 0.02;

        // Range rings.
        for (int k = 1; k <= 4; k++) {
            float rr = scopeR * float(k) / 4.0;
            col += phosphor * sn_ring(r, rr, 0.0032) * 0.30;
        }
        col += phosphor * sn_ring(r, scopeR, 0.004) * 0.85;

        // Bearing spokes.
        for (int k = 0; k < 8; k++) {
            float sa = float(k) / 8.0 * 6.28318531;
            float da = sn_wrap(ang - sa);
            col += phosphor * smoothstep(0.05, 0.0, abs(da) * r) * 0.22;
        }

        // Rotating sweep beam: bright leading edge, soft trailing wedge.
        float sd = sn_wrap(ang - sweep);
        col += phosphor * smoothstep(0.035, 0.0, abs(sd)) * 1.5;
        float behind = clamp(-sd / 1.15, 0.0, 1.0);
        float trail = pow(1.0 - behind, 3.0) * step(sd, 0.0);
        col += phosphor * trail * 0.55;

        // Spectrum blips: angle = bucket index, radius = bucket energy.
        for (int i = 0; i < 24; i++) {
            float fi = float(i);
            float ba = fi / 24.0 * 6.28318531;
            float e = spec(fi / 24.0);
            vec2 bp = vec2(cos(ba), sin(ba)) * (0.06 + e * (scopeR - 0.10));
            float d = length(c - bp);
            float blip = smoothstep(0.03 + e * 0.02, 0.0, d) * (0.35 + e * 1.3);
            vec3 bc = (uRainbow == 1)
                ? hsv2rgb(vec3(fract(0.30 + fi * 0.02 + uTime * 0.015), 0.85, 1.0))
                : phosphor;
            col += bc * blip;
        }

        // Beat ping: bright ring expanding out from the core, gated by the
        // beat-phase sawtooth (falls back to a free-running pulse if the
        // deck never locks a tempo).
        float pingPh = (uBpm > 1.0) ? uPhase : fract(uTime * 0.6);
        float ping = sn_ring(r, pingPh * scopeR * 0.95, 0.05) * (1.0 - pingPh);
        col += phosphor * ping * (0.5 + uPulse * 1.6);

        // Fine CRT grain, brighter with treble.
        col += phosphor * (vnoise(c * 44.0 + uTime * 2.2) - 0.5) * 0.035 * (0.25 + uTreb);
    } else {
        // Outer bezel glow just past the rim.
        col += phosphor * sn_ring(r, scopeR, 0.012) * 0.14;
    }

    // Scanlines.
    float sl = sin(vUv.y * uRes.y * 1.5) * 0.5 + 0.5;
    col *= 0.86 + 0.14 * sl;

    // CRT vignette.
    float vig = smoothstep(0.86, 0.15, length(vUv - 0.5));
    col *= vig;

    col += prev;
    col = min(col, vec3(1.2));

    frag = vec4(col, 1.0);
}"#;

const FRAG_PULSAR: &str = r#"
// gl_pulsar — spinning neutron star. Blinding core, two opposed lighthouse
// beams sweeping once per beat (locked to uPhase), a rippling accretion
// glow disc that breathes with bass, a fixed twinkling starfield, and
// crest-triggered radial shockwaves. Beam sweeps leave a feedback afterglow.

float pu_wrap(float a) {
    return mod(a + 3.14159265, 6.28318531) - 3.14159265;
}

// Volumetric lighthouse beam: wide near the core, narrowing with distance,
// soft angular falloff on both edges.
float pu_beam(vec2 c, float beamAngle) {
    float r = length(c);
    float ang = atan(c.y, c.x);
    float da = pu_wrap(ang - beamAngle);
    float halfWidth = 0.05 + 0.11 / (r + 0.16);
    float falloff = smoothstep(halfWidth, 0.0, abs(da));
    float reach = smoothstep(1.15, 0.05, r);
    return falloff * reach;
}

void main() {
    vec2 c = aspect(vUv);
    float r = length(c);

    // Afterglow: beams and shockwaves painted here decay slowly, biased
    // down with a floor subtraction so nothing can pile up to blown white.
    vec3 prev = texture(uPrev, vUv).rgb;
    prev = max(prev * 0.90 - 0.008, 0.0);

    vec3 tint = (uRainbow == 1)
        ? hsv2rgb(vec3(fract(0.56 + uBright * 0.3 + uTime * 0.01), 0.6, 1.0))
        : uAccent;

    vec3 col = vec3(0.0);

    // Background starfield: fixed positions, gentle twinkle, brighter treble.
    for (int i = 0; i < 32; i++) {
        float fi = float(i);
        vec2 sp = (vec2(hash(vec2(fi, 11.0)), hash(vec2(fi, 47.0))) - 0.5) * 1.6;
        float tw = 0.4 + 0.6 * sin(uTime * (0.5 + hash(vec2(fi, 3.0)) * 1.3) + fi * 5.1);
        float d = length(c - sp);
        col += vec3(0.82, 0.88, 1.0) * smoothstep(0.011, 0.0, d) * max(tw, 0.0) * (0.4 + uTreb * 0.6);
    }

    // Accretion glow disc: rippling annulus, breathes with bass energy.
    float discR = 0.15 + uBass * 0.09;
    float ripple = sin(r * 42.0 - uTime * 3.5 - uBass * 6.0) * 0.5 + 0.5;
    float disc = smoothstep(discR + 0.24, discR, r) * smoothstep(discR * 0.5, discR, r);
    disc *= (0.30 + 0.70 * ripple) * (0.45 + uMid * 0.85);
    col += tint * disc;

    // Lighthouse beams: two opposed sweeps, one full revolution per beat.
    float beamAngle = uPhase * 6.28318531;
    float b1 = pu_beam(c, beamAngle);
    float b2 = pu_beam(c, beamAngle + 3.14159265);
    float beamE = 0.45 + uMid * 0.7 + uPulse * 0.7;
    col += tint * (b1 + b2) * beamE;
    col += vec3(1.0) * (pow(b1, 5.0) + pow(b2, 5.0)) * 0.7;

    // Crest-triggered shockwaves: always ticking softly, crest spikes flare
    // them bright (silence still animates via uTime).
    for (int k = 0; k < 3; k++) {
        float ph = fract(uTime * 0.32 + float(k) / 3.0);
        float ring = smoothstep(0.022, 0.0, abs(r - ph * 0.95)) * (1.0 - ph);
        col += tint * ring * (0.12 + uCrest * 1.7);
    }

    // Neutron star core: tiny, blinding, pulses with bass + the beat.
    float glow = exp(-r * r * 46.0) * (0.55 + uBass * 0.55 + uPulse * 0.7);
    col += mix(tint, vec3(1.0), 0.7) * glow;
    col += vec3(1.0) * smoothstep(0.022, 0.0, r) * (0.75 + uPulse * 0.6);

    col += prev;
    col = min(col, vec3(1.18));

    frag = vec4(col, 1.0);
}"#;

const FRAG_BLACKHOLE: &str = r#"
// gl_blackhole -- accretion disk with inward-lensing feedback.
// Central event horizon (pure black), a thin photon ring, and a fast
// orbiting accretion band built from polar noise streaks with a
// doppler-bright side. uPrev is sampled through a radial pinch toward the
// center so trails smear inward like light bending around the hole.

vec3 bh_firePal(float h) {
    h = clamp(h, 0.0, 1.0);
    return clamp(vec3(h * 1.9, h * h * 1.15, h * h * h * 0.55), 0.0, 1.0);
}

float bh_streaks(vec2 p) {
    float v = 0.0;
    float a = 0.6;
    for (int i = 0; i < 4; i++) {
        v += a * vnoise(p);
        p = p * vec2(1.9, 1.35) + vec2(3.1, 1.7);
        a *= 0.55;
    }
    return v;
}

void main() {
    vec2 c = aspect(vUv);
    float r = length(c) + 1e-5;
    float ang = atan(c.y, c.x);

    // Gravitational lensing: pinch the previous frame's sample radially
    // toward the center so everything smears inward. uPulse deepens the
    // pinch for a brief inward lurch on every beat (the gulp).
    float lurch = uPulse * 0.16;
    float lensK = 0.05 + uBass * 0.06 + lurch;
    float rl = max(r - lensK / (r + 0.22), 0.0015);
    vec2 w = (c / r) * rl;
    w.x /= uRes.x / uRes.y;
    vec3 prevCol = texture(uPrev, w + 0.5).rgb;
    // Bias feedback down hard so lensed trails can never pile to white.
    prevCol = max(prevCol * (0.966 - uPulse * 0.01) - 0.006, 0.0);

    // Event horizon + photon ring geometry. Bass widens the whole disk.
    float horizon = 0.15 + uBass * 0.05;
    float diskW = 0.34 + uBass * 0.12;
    float photon = smoothstep(0.012, 0.0, abs(r - horizon - 0.006));

    // Accretion ring: fast orbiting polar noise streaks. uFlux (spectral
    // onsets) kicks the orbit speed up on transients.
    float orbitT = uTime * (1.6 + uBass * 1.4 + uFlux * 2.2);
    vec2 pp = vec2(ang * 1.6 + orbitT, (r - horizon) * 7.0);
    float streak = bh_streaks(pp);
    float x = r - horizon;
    float band = smoothstep(0.0, 0.05, x) * (1.0 - smoothstep(diskW * 0.7, diskW, x));

    // Doppler beaming: relativistic brightening on the approaching side.
    float doppler = 0.35 + 0.85 * (0.5 + 0.5 * cos(ang - 0.4));

    // Spectrum hot streaks riding the ring angle.
    float s = spec(fract(ang / 6.28318 + 0.5 + uPhase * 0.4));
    float hot = smoothstep(0.3, 1.0, s) * band;

    float flash = uPulse * 0.55;
    float heat = clamp(band * streak * doppler * (0.8 + flash) + hot * 0.6, 0.0, 1.3);

    vec3 diskCol;
    if (uRainbow == 1) {
        float hue = fract(ang / 6.28318 + uTime * 0.05 + heat * 0.12);
        diskCol = hsv2rgb(vec3(hue, 0.7, clamp(heat, 0.0, 1.0)));
    } else {
        vec3 fire = bh_firePal(heat);
        diskCol = mix(fire, uAccent * (0.6 + heat * 0.6), 0.4);
    }

    vec3 photonCol = (uRainbow == 1)
        ? hsv2rgb(vec3(fract(uTime * 0.04), 0.25, 1.0))
        : mix(uAccent, vec3(1.0), 0.6);

    vec3 col = diskCol * heat + photonCol * photon * (1.3 + flash + uLoud * 0.4);

    // Event horizon: pure black disc, nothing escapes -- masks both the
    // fresh ring light and the lensed feedback trail.
    float holeMask = smoothstep(horizon - 0.015, horizon, r);
    col *= holeMask;
    prevCol *= holeMask;

    vec3 outCol = max(col, prevCol);
    frag = vec4(min(outCol, vec3(1.4)), 1.0);
}"#;

const FRAG_OCEAN: &str = r#"
// gl_ocean -- moonlit ocean. Layered sine + fbm swell scrolling toward the
// viewer with mode7-style perspective (closer = lower on screen = bigger),
// a moon disc with a glitter reflection path, and stars above the horizon.

float ocn_waves(vec2 w, float t) {
    float h = 0.0;
    h += sin(w.x * 1.3 + w.y * 0.55 + t * 1.6) * 0.5;
    h += sin(w.x * 2.6 - w.y * 1.15 + t * 2.3) * 0.28;
    h += sin(w.x * 4.1 + w.y * 2.4 - t * 3.1) * 0.14;
    h += (fbm(w * 0.55 + vec2(0.0, t * 0.25)) - 0.5) * 0.7;
    return h;
}

void main() {
    vec2 c = aspect(vUv);
    float horizonY = 0.52;

    // Persistence trail: sparkle glints and the moon halo bloom softly
    // instead of popping frame to frame. Biased down hard so it can never
    // pile to white.
    vec3 prevCol = texture(uPrev, vUv).rgb;
    prevCol = max(prevCol * 0.90 - 0.012, 0.0);

    vec2 moonC = vec2(0.30, 0.30);
    float moonD = length(c - moonC);
    float moonDisc = smoothstep(0.085, 0.076, moonD);
    float moonHaloBase = smoothstep(0.42, 0.0, moonD);
    float moonHalo = moonHaloBase * (0.30 + uLoud * 0.6) * (0.85 + 0.15 * sin(uPhase * 6.28318));

    vec3 moonCol = (uRainbow == 1)
        ? hsv2rgb(vec3(0.58 + sin(uTime * 0.05) * 0.03, 0.18, 1.0))
        : vec3(0.85, 0.90, 1.0);

    vec3 scene;
    if (vUv.y > horizonY) {
        // ---- sky: stars + moon ----
        vec2 starGrid = floor(vUv * vec2(140.0, 90.0));
        float starRand = hash(starGrid);
        float starTwinkle = 0.5 + 0.5 * sin(uTime * 3.0 + starRand * 41.0);
        float star = step(0.986, starRand) * starTwinkle;

        vec3 skyCol = mix(vec3(0.010, 0.016, 0.040), vec3(0.03, 0.05, 0.10),
                           smoothstep(horizonY, horizonY + 0.4, vUv.y));
        skyCol += vec3(0.75, 0.80, 0.90) * star;
        skyCol += moonCol * (moonDisc * 1.3 + moonHalo);
        scene = skyCol;
    } else {
        // ---- ocean: perspective wave field, closer = lower + bigger ----
        float sy = clamp((horizonY - vUv.y) / horizonY, 0.0, 1.0);
        float depth = 0.30 / (sy + 0.045);
        vec2 world = vec2(c.x * depth * 2.2, depth * 2.0 + uTime * 0.55);

        float lowSpec = spec(0.02) + spec(0.06) + spec(0.10);
        float waveAmp = 0.45 + uBass * 0.7 + lowSpec * 0.2;
        // A beat sends a gentle swell riding through the wave field.
        float swell = uPulse * 0.5 * sin(world.y * 0.9 - uTime * 2.2);
        float h = ocn_waves(world, uTime * 0.8) * waveAmp + swell;

        vec3 deep = vec3(0.008, 0.026, 0.075);
        vec3 shallow = vec3(0.05, 0.13, 0.27);
        vec3 waterCol = mix(deep, shallow, sy);
        waterCol = mix(vec3(0.012, 0.02, 0.05), waterCol, smoothstep(0.0, 0.3, sy));

        vec3 waterTint = (uRainbow == 1)
            ? hsv2rgb(vec3(0.55 + h * 0.05 + uTime * 0.01, 0.55, 1.0))
            : uAccent;
        float crestMix = (uRainbow == 1) ? 0.5 : 0.32;
        float crest = smoothstep(0.12, 0.55, h);
        waterCol += waterTint * crest * crestMix;

        // Broad twinkle across the whole surface, treble-driven flicker.
        float twGrid = hash(floor(world * 9.0 + uTime * 1.5));
        float sparkle = step(0.965 - uTreb * 0.22, twGrid) * (0.4 + crest * 0.6);
        waterCol += vec3(0.55, 0.65, 0.8) * sparkle * 0.5;

        // Moonlight glitter path straight down the water toward the
        // camera, widening as it nears the viewer; uWidth breathes it.
        float pathWidth = (0.018 + sy * 0.06) * (0.85 + uWidth * 0.15);
        float path = smoothstep(pathWidth, 0.0, abs(c.x - moonC.x));
        float pathTwinkle = hash(floor(vec2(world.x * 14.0, world.y * 3.0 + uTime * 5.0)));
        float pathSparkle = step(0.93 - uTreb * 0.3, pathTwinkle);
        waterCol += moonCol * path * (0.25 + pathSparkle * 1.6) * (0.5 + uLoud * 0.4);

        scene = waterCol;
    }

    vec3 outCol = max(scene, prevCol);
    frag = vec4(min(outCol, vec3(1.3)), 1.0);
}"#;

const FRAG_PRESENT: &str = r#"#version 330 core
in vec2 vUv;
out vec4 frag;
uniform sampler2D uTex;
void main() { frag = vec4(texture(uTex, vUv).rgb, 1.); }
"#;

/// Stackable post-fx uber-shader. Runs ONCE, over the mode's already-resolved
/// feedback-pass result (`uTex`), never touching the feedback loop itself.
/// `uFx` is a bitmask; every set bit's block runs, in bit order, each acting
/// on whatever `uv`/`col` the earlier blocks left behind — that's what makes
/// combinations stack instead of needing N*N dedicated shader variants.
const FRAG_POST: &str = r#"#version 330 core
in vec2 vUv;
out vec4 frag;
uniform sampler2D uTex;
uniform int   uFx;
uniform vec2  uRes;
uniform float uTime;
uniform float uPulse;
uniform float uBass;
uniform float uPhase;
uniform float uLoud;
uniform vec3  uAccent;

float post_hash(vec2 p) {
    p = fract(p * vec2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

void main() {
    vec2 uv = vUv;

    // bit 0 MIRROR: four-quadrant kaleido mirror (abs uv around center).
    if ((uFx & 1) != 0) {
        uv = abs(uv - 0.5) + 0.5;
    }
    uv = clamp(uv, 0.0, 1.0);
    vec3 col = texture(uTex, uv).rgb;

    // bit 1 ZOOMBLUR: 5-tap radial zoom blur toward center.
    if ((uFx & 2) != 0) {
        float strength = 0.02 + uPulse * 0.03;
        vec3 sum = vec3(0.0);
        for (int i = 0; i < 5; i++) {
            float t = float(i) * 0.25 * strength;
            sum += texture(uTex, clamp(mix(uv, vec2(0.5), t), 0.0, 1.0)).rgb;
        }
        col = sum * 0.2;
    }

    // bit 2 ABERRATION: RGB channel split along the radial direction.
    if ((uFx & 4) != 0) {
        vec2 dir = normalize(uv - vec2(0.5) + 1e-4);
        float off = 0.002 + uPulse * 0.006;
        float rC = texture(uTex, clamp(uv + dir * off, 0.0, 1.0)).r;
        float gC = texture(uTex, uv).g;
        float bC = texture(uTex, clamp(uv - dir * off, 0.0, 1.0)).b;
        col = vec3(rC, gC, bC);
    }

    // bit 3 PIXELATE: quantize uv to a grid (~140 cells wide; fewer, i.e.
    // chunkier blocks, on bass spikes).
    if ((uFx & 8) != 0) {
        float cellsX = max(140.0 - uBass * 90.0, 20.0);
        float cellsY = max(cellsX * (uRes.y / max(uRes.x, 1.0)), 8.0);
        vec2 g = uv * vec2(cellsX, cellsY);
        uv = clamp((floor(g) + 0.5) / vec2(cellsX, cellsY), 0.0, 1.0);
        col = texture(uTex, uv).rgb;
    }

    // bit 4 HALFTONE: brightness -> dot pattern in a rotated grid, hue kept.
    if ((uFx & 16) != 0) {
        float lum = dot(col, vec3(0.299, 0.587, 0.114));
        float ang = 0.7854;
        float ca = cos(ang), sa = sin(ang);
        vec2 rp = mat2(ca, -sa, sa, ca) * (uv * uRes);
        vec2 cell = fract(rp / 8.0) - 0.5;
        float rad = clamp(sqrt(lum), 0.0, 1.0) * 0.5;
        float dotMask = smoothstep(rad, rad - 0.12, length(cell));
        vec3 hue = (lum > 1e-3) ? col / lum : vec3(0.0);
        col = hue * lum * 1.3 * dotMask;
    }

    // bit 5 SCANLINES: CRT scanlines + subtle barrel curvature + RGB mask.
    if ((uFx & 32) != 0) {
        vec2 c = uv - 0.5;
        float r2 = dot(c, c);
        vec2 buv = clamp(uv + c * r2 * 0.15, 0.0, 1.0);
        col = texture(uTex, buv).rgb;
        float sl = sin(uv.y * uRes.y * 1.5) * 0.5 + 0.5;
        col *= 0.82 + 0.18 * sl;
        float m = mod(gl_FragCoord.x, 3.0);
        vec3 mask = (m < 1.0) ? vec3(1.05, 0.92, 0.92)
                  : (m < 2.0) ? vec3(0.92, 1.05, 0.92)
                              : vec3(0.92, 0.92, 1.05);
        col *= mix(vec3(1.0), mask, 0.25);
    }

    // bit 6 GRAIN: film grain (hash per pixel per frame) + corner vignette.
    if ((uFx & 64) != 0) {
        float g = post_hash(gl_FragCoord.xy + fract(uTime) * 971.31) - 0.5;
        col += g * 0.06;
        float vig = smoothstep(0.95, 0.35, length(uv - 0.5));
        col *= mix(0.55, 1.0, vig);
    }

    // bit 7 STROBE: brief white lift right when uPhase wraps, scaled by loud.
    if ((uFx & 128) != 0) {
        float amt = (1.0 - smoothstep(0.0, 0.06, uPhase)) * uLoud;
        col = mix(col, vec3(1.0), amt * 0.8);
    }

    // bit 8 EDGEGLOW: 4-tap luminance edge detect as a neon outline.
    if ((uFx & 256) != 0) {
        vec2 px = 1.0 / uRes;
        float lL = dot(texture(uTex, clamp(uv - vec2(px.x, 0.0), 0.0, 1.0)).rgb, vec3(0.299, 0.587, 0.114));
        float lR = dot(texture(uTex, clamp(uv + vec2(px.x, 0.0), 0.0, 1.0)).rgb, vec3(0.299, 0.587, 0.114));
        float lU = dot(texture(uTex, clamp(uv + vec2(0.0, px.y), 0.0, 1.0)).rgb, vec3(0.299, 0.587, 0.114));
        float lD = dot(texture(uTex, clamp(uv - vec2(0.0, px.y), 0.0, 1.0)).rgb, vec3(0.299, 0.587, 0.114));
        float edge = clamp((abs(lL - lR) + abs(lU - lD)) * 4.0, 0.0, 1.0);
        col += uAccent * edge * 1.4;
    }

    // bit 9 THERMAL: luminance -> thermal palette remap (black-blue-magenta-
    // orange-yellow-white).
    if ((uFx & 512) != 0) {
        float lum = clamp(dot(col, vec3(0.299, 0.587, 0.114)), 0.0, 1.0);
        vec3 c0 = vec3(0.0, 0.0, 0.0);
        vec3 c1 = vec3(0.0, 0.0, 0.6);
        vec3 c2 = vec3(0.65, 0.0, 0.65);
        vec3 c3 = vec3(1.0, 0.4, 0.0);
        vec3 c4 = vec3(1.0, 0.9, 0.0);
        vec3 c5 = vec3(1.0, 1.0, 1.0);
        vec3 tc;
        if (lum < 0.2)       tc = mix(c0, c1, lum / 0.2);
        else if (lum < 0.45) tc = mix(c1, c2, (lum - 0.2) / 0.25);
        else if (lum < 0.7)  tc = mix(c2, c3, (lum - 0.45) / 0.25);
        else if (lum < 0.88) tc = mix(c3, c4, (lum - 0.7) / 0.18);
        else                 tc = mix(c4, c5, (lum - 0.88) / 0.12);
        col = tc;
    }

    frag = vec4(clamp(col, 0.0, 1.6), 1.0);
}"#;

/// Fragment sources in `GlMode` order.
const PROGRAM_SRC: [&str; MODE_COUNT] = [
    FRAG_WARP,
    FRAG_FLAME,
    FRAG_SMOKE,
    FRAG_PLASMA,
    FRAG_STARFIELD,
    FRAG_KALEIDO,
    FRAG_TUNNEL3D,
    FRAG_METABALLS,
    FRAG_VORONOI,
    FRAG_NEBULA,
    FRAG_TERRAIN,
    FRAG_RIPPLES,
    FRAG_JULIA,
    FRAG_MATRIX,
    FRAG_SCOPERING,
    FRAG_SKYBOX,
    FRAG_AURORA,
    FRAG_OUTRUN,
    FRAG_CITY,
    FRAG_WORMHOLE,
    FRAG_SPIRO,
    FRAG_LASER,
    FRAG_DISCOBALL,
    FRAG_HEXGRID,
    FRAG_LIGHTNING,
    FRAG_DNA,
    FRAG_BUBBLES,
    FRAG_COPPER,
    FRAG_LEDWALL,
    FRAG_SONAR,
    FRAG_PULSAR,
    FRAG_BLACKHOLE,
    FRAG_OCEAN,
];

impl GlStage {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        unsafe {
            let compile = |vs: &str, fs: &str| -> Result<glow::Program, String> {
                let program = gl.create_program()?;
                let stages = [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)];
                let mut shaders = Vec::new();
                for (kind, src) in stages {
                    let sh = gl.create_shader(kind)?;
                    gl.shader_source(sh, src);
                    gl.compile_shader(sh);
                    if !gl.get_shader_compile_status(sh) {
                        return Err(gl.get_shader_info_log(sh));
                    }
                    gl.attach_shader(program, sh);
                    shaders.push(sh);
                }
                gl.link_program(program);
                if !gl.get_program_link_status(program) {
                    return Err(gl.get_program_info_log(program));
                }
                for sh in shaders {
                    gl.detach_shader(program, sh);
                    gl.delete_shader(sh);
                }
                Ok(program)
            };

            let mut progs: Vec<glow::Program> = Vec::with_capacity(MODE_COUNT);
            for (i, frag) in PROGRAM_SRC.iter().enumerate() {
                progs.push(
                    compile(VS_FULL, &format!("{FRAG_COMMON}{frag}"))
                        .map_err(|e| format!("mode {i}: {e}"))?,
                );
            }
            let prog: [glow::Program; MODE_COUNT] =
                progs.try_into().map_err(|_| "prog array".to_string())?;
            let present = compile(VS_FULL, FRAG_PRESENT)?;
            let post = compile(VS_FULL, FRAG_POST)?;
            let vao = gl.create_vertex_array()?;

            let mk_tex = || -> Result<glow::Texture, String> {
                let t = gl.create_texture()?;
                gl.bind_texture(glow::TEXTURE_2D, Some(t));
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);
                Ok(t)
            };

            let spec_tex = mk_tex()?;
            gl.tex_image_2d(
                glow::TEXTURE_2D, 0, glow::R32F as i32, SPEC_W as i32, 1, 0,
                glow::RED, glow::FLOAT, glow::PixelUnpackData::Slice(None),
            );
            let wave_tex = mk_tex()?;
            gl.tex_image_2d(
                glow::TEXTURE_2D, 0, glow::R32F as i32, WAVE_W as i32, 1, 0,
                glow::RED, glow::FLOAT, glow::PixelUnpackData::Slice(None),
            );
            let hist_tex = mk_tex()?;
            let zeros = vec![0u8; SPEC_W * HIST_ROWS * 4];
            gl.tex_image_2d(
                glow::TEXTURE_2D, 0, glow::R32F as i32, SPEC_W as i32, HIST_ROWS as i32, 0,
                glow::RED, glow::FLOAT, glow::PixelUnpackData::Slice(Some(&zeros)),
            );

            let fbo = [gl.create_framebuffer()?, gl.create_framebuffer()?];
            let tex = [mk_tex()?, mk_tex()?];
            let post_fbo = gl.create_framebuffer()?;
            let post_tex = mk_tex()?;
            gl.bind_texture(glow::TEXTURE_2D, None);

            Ok(GlStage {
                prog,
                present,
                post,
                vao,
                fbo,
                tex,
                post_fbo,
                post_tex,
                size: (0, 0),
                spec_tex,
                wave_tex,
                hist_tex,
                hist_row: 0,
                frame: 0,
                ping: 0,
                sim: 0.0,
                last_paint: None,
            })
        }
    }

    /// (Re)size the ping-pong feedback textures to the canvas pixel size.
    unsafe fn ensure_size(&mut self, gl: &glow::Context, w: i32, h: i32) { unsafe {
        if (w, h) == self.size || w <= 0 || h <= 0 {
            return;
        }
        for i in 0..2 {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.tex[i]));
            gl.tex_image_2d(
                glow::TEXTURE_2D, 0, glow::RGBA16F as i32, w, h, 0,
                glow::RGBA, glow::FLOAT, glow::PixelUnpackData::Slice(None),
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo[i]));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D,
                Some(self.tex[i]), 0,
            );
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
        // Third target for the post-fx pass, sized with the ping-pong pair.
        // Kept fully separate from tex[0]/tex[1] so post-fx can never feed
        // back into the mode's own feedback loop.
        gl.bind_texture(glow::TEXTURE_2D, Some(self.post_tex));
        gl.tex_image_2d(
            glow::TEXTURE_2D, 0, glow::RGBA16F as i32, w, h, 0,
            glow::RGBA, glow::FLOAT, glow::PixelUnpackData::Slice(None),
        );
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.post_fbo));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D,
            Some(self.post_tex), 0,
        );
        gl.clear_color(0.0, 0.0, 0.0, 0.0);
        gl.clear(glow::COLOR_BUFFER_BIT);
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        self.size = (w, h);
    }}

    /// One feedback pass + present, inside egui's paint callback. `vp` is
    /// egui_glow's own viewport for this callback: [left_px, from_bottom_px,
    /// width_px, height_px] (GL bottom-origin).
    pub fn paint(&mut self, gl: &glow::Context, u: &Uniforms, vp: [i32; 4]) {
        unsafe {
            let (w, h) = (vp[2], vp[3]);
            if w < 8 || h < 8 {
                return;
            }
            self.frame = self.frame.wrapping_add(1);
            // Sim clock: clamp dt so frame stalls never fast-forward the FX.
            let now = Instant::now();
            let dt = self
                .last_paint
                .map(|t| now.duration_since(t).as_secs_f32())
                .unwrap_or(1.0 / 60.0)
                .min(0.05);
            self.last_paint = Some(now);
            self.sim += dt;
            let prev_fbo = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
            self.ensure_size(gl, w, h);

            // Upload audio textures (spectrum, waveform, and every other frame
            // one new row of the scrolling spectrum-history for the terrain).
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.spec_tex));
            let spec_bytes: &[u8] =
                core::slice::from_raw_parts(u.spec.as_ptr() as *const u8, SPEC_W * 4);
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D, 0, 0, 0, SPEC_W as i32, 1,
                glow::RED, glow::FLOAT, glow::PixelUnpackData::Slice(Some(spec_bytes)),
            );
            gl.active_texture(glow::TEXTURE2);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.wave_tex));
            let wave_bytes: &[u8] =
                core::slice::from_raw_parts(u.wave.as_ptr() as *const u8, WAVE_W * 4);
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D, 0, 0, 0, WAVE_W as i32, 1,
                glow::RED, glow::FLOAT, glow::PixelUnpackData::Slice(Some(wave_bytes)),
            );
            gl.active_texture(glow::TEXTURE3);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.hist_tex));
            if self.frame % 2 == 0 {
                gl.tex_sub_image_2d(
                    glow::TEXTURE_2D, 0, 0, self.hist_row as i32, SPEC_W as i32, 1,
                    glow::RED, glow::FLOAT, glow::PixelUnpackData::Slice(Some(spec_bytes)),
                );
                self.hist_row = (self.hist_row + 1) % HIST_ROWS;
            }

            // Offscreen feedback pass. Scissor must be OFF here (egui's window
            // scissor would clip the FBO), blend off (we own every pixel).
            let src = self.ping;
            let dst = 1 - self.ping;
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::BLEND);
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo[dst]));
            gl.viewport(0, 0, w, h);
            let prog = self.prog[u.mode as usize];
            gl.use_program(Some(prog));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.tex[src]));
            let loc = |n: &str| gl.get_uniform_location(prog, n);
            gl.uniform_1_i32(loc("uPrev").as_ref(), 0);
            gl.uniform_1_i32(loc("uSpec").as_ref(), 1);
            gl.uniform_1_i32(loc("uWave").as_ref(), 2);
            gl.uniform_1_i32(loc("uHist").as_ref(), 3);
            gl.uniform_1_f32(loc("uHistRow").as_ref(), self.hist_row as f32 / HIST_ROWS as f32);
            gl.uniform_2_f32(loc("uRes").as_ref(), w as f32, h as f32);
            gl.uniform_1_f32(loc("uTime").as_ref(), self.sim);
            gl.uniform_1_f32(loc("uBass").as_ref(), u.bass);
            gl.uniform_1_f32(loc("uMid").as_ref(), u.mid);
            gl.uniform_1_f32(loc("uTreb").as_ref(), u.treble);
            gl.uniform_1_f32(loc("uPulse").as_ref(), u.pulse);
            gl.uniform_1_f32(loc("uPhase").as_ref(), u.beat_phase);
            gl.uniform_1_f32(loc("uBright").as_ref(), u.bright);
            gl.uniform_1_i32(loc("uRainbow").as_ref(), if u.rainbow { 1 } else { 0 });
            gl.uniform_3_f32(loc("uAccent").as_ref(), u.accent[0], u.accent[1], u.accent[2]);
            gl.uniform_1_f32(loc("uBpm").as_ref(), u.bpm);
            gl.uniform_1_f32(loc("uBpmConf").as_ref(), u.bpm_conf);
            gl.uniform_1_f32(loc("uFlux").as_ref(), u.flux);
            gl.uniform_1_f32(loc("uLoud").as_ref(), u.loud);
            gl.uniform_1_f32(loc("uCrest").as_ref(), u.crest);
            gl.uniform_1_f32(loc("uCorr").as_ref(), u.corr);
            gl.uniform_1_f32(loc("uWidth").as_ref(), u.width);
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);

            // Stackable post-fx pass: reads the mode's just-rendered result
            // from tex[dst] and writes into the SEPARATE post_fbo/post_tex —
            // never back into tex[dst] itself, so the feedback loop that mode
            // shaders read next frame (uPrev) stays pure and untouched by
            // post-fx. Skipped entirely when no bit is set (present reads
            // tex[dst] directly below, zero extra cost).
            let present_src = if u.fx != 0 {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.post_fbo));
                gl.viewport(0, 0, w, h);
                gl.use_program(Some(self.post));
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(self.tex[dst]));
                let ploc = |n: &str| gl.get_uniform_location(self.post, n);
                gl.uniform_1_i32(ploc("uTex").as_ref(), 0);
                gl.uniform_1_i32(ploc("uFx").as_ref(), u.fx as i32);
                gl.uniform_2_f32(ploc("uRes").as_ref(), w as f32, h as f32);
                gl.uniform_1_f32(ploc("uTime").as_ref(), self.sim);
                gl.uniform_1_f32(ploc("uPulse").as_ref(), u.pulse);
                gl.uniform_1_f32(ploc("uBass").as_ref(), u.bass);
                gl.uniform_1_f32(ploc("uPhase").as_ref(), u.beat_phase);
                gl.uniform_1_f32(ploc("uLoud").as_ref(), u.loud);
                gl.uniform_3_f32(ploc("uAccent").as_ref(), u.accent[0], u.accent[1], u.accent[2]);
                gl.bind_vertex_array(Some(self.vao));
                gl.draw_arrays(glow::TRIANGLES, 0, 3);
                self.post_tex
            } else {
                self.tex[dst]
            };

            // Present: restore egui's framebuffer + ITS viewport (the callback
            // rect) + scissor, then a plain fullscreen triangle fills exactly
            // the canvas rect and clips exactly to egui's clip rect.
            gl.bind_framebuffer(glow::FRAMEBUFFER, if prev_fbo != 0 {
                Some(glow::NativeFramebuffer(
                    std::num::NonZeroU32::new(prev_fbo as u32).unwrap(),
                ))
            } else {
                None
            });
            gl.viewport(vp[0], vp[1], vp[2], vp[3]);
            gl.enable(glow::SCISSOR_TEST);
            gl.use_program(Some(self.present));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(present_src));
            gl.uniform_1_i32(gl.get_uniform_location(self.present, "uTex").as_ref(), 0);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_vertex_array(None);

            self.ping = dst;
        }
    }
}
