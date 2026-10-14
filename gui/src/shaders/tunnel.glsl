void main() {
    vec2 p = aspect(vUv) / (1. + uWidth*.12) - vec2(sin(uTime*.25),cos(uTime*.19))*.035*uWidth;
    float r = length(p);
    float a = atan(p.y, p.x);
    float depth = -log(max(r,.002));
    float twist = uControls.z * (.16 * depth + uTime*.13 + uMid*.12);
    float angle = a + twist;
    // Mirrored frequency mapping closes the angular seam without a jump.
    float frequency = abs(sin(angle*.5));
    float spectrum = spec(frequency);
    float contour = 1. + uControls.y * (.22*(spectrum-.35) + .07*wav(frequency));
    contour += .055*sin(angle*6. + uMid*2.);
    float kick = uPulse*.65 + uFlux*.16 + uBass*.28;
    float z = -log(max(r*contour,.002)) * (1.0 + uControls.x*1.5) - uTime*.72 - kick;
    float d = abs(fract(z)-.5);
    float aa = max(fwidth(z),.005);
    float wire = 1.-smoothstep(.008+uPulse*.015,.008+uPulse*.015+aa*1.5,d);
    float halo = exp(-d * (24.-uTreb*8.)) * (.18+uFlux*.6);
    float gate = smoothstep(.012,.08,r);
    vec3 ink = uRainbow==1 ? hsv2rgb(vec3(a/6.283+depth*.09+uTime*.025,.78,1.)) : uAccent;
    vec3 col = ink*(wire+halo)*gate*(.45+spectrum*.9+uPulse*.75);
    // Longitudinal ribs join the rings into a passage.
    float rib = pow(max(0.,cos(angle*24.)),32.) * (.025+uTreb*.055) * gate;
    col += ink*rib;
    float sparks = pow(max(0., sin(angle*79.+floor(depth*8.)*13.)),64.)
        * pow(max(0.,sin(depth*31.-uTime*9.)),16.);
    col += mix(ink,vec3(1.),.7)*sparks*uTreb*gate*.8;
    vec2 uv=(vUv-.5)/(1.006+uPulse*.01)+.5;
    vec3 previous=texture(uPrev,uv).rgb*mix(.0,.88,uControls.w*.5);
    col=max(col,previous);
    frag=vec4(min(col,vec3(1.15)),1.);
}
