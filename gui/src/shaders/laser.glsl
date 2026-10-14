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
    float fanSpread = (0.14 + uMid * 0.30) * (.4 + uControls.x * .6);
    float strobe = 0.40 + uPulse * 0.65 + uFlux * 0.35;

    for (int oi = 0; oi < 3; oi++) {
        vec2 org = (oi == 0) ? org0 : (oi == 1) ? org1 : org2;
        float baseA = (oi == 0) ? (0.30 * 3.14159) : (oi == 1) ? (0.70 * 3.14159) : (-0.5 * 3.14159);
        float sweepDir = (oi == 0) ? 1.0 : (oi == 1) ? -1.0 : 1.3;
        float sweep = sin(uTime * .65 * sweepDir + float(oi) * 2.1 + uPulse * .12) * fanSpread;
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
            float beamWidth = .0015 + .0035 * uControls.y;
            float core = exp(-d * d / (beamWidth * beamWidth));
            float halo = exp(-d * 75.0) * 0.10;
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
        col += sHue * sCore * sAtten * uTreb * 0.65;
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
    col += hazeCol * fogN * (0.015 + lum * 0.10) * uControls.z;

    // feedback trail: hard bias down so it can never pile to blown white
    vec3 prev = texture(uPrev, vUv).rgb * mix(.20,.88,uControls.w*.5) - 0.003;
    prev = max(prev, vec3(0.0));
    col = 1.0 - exp(-max(col,vec3(0.0))*3.0);
    col = max(col, prev);

    col = max(col, vec3(0.0));
    frag = vec4(col, 0.0);
}
