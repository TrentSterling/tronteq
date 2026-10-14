float hgt(vec2 xz) {
    // Map a broad landscape to FFT bins. History already scrolls in time.
    float f = clamp(xz.x * .13 + .5, .015, .985);
    float age = clamp(xz.y / 14., .008, .98);
    float energy = hist(vec2(f, age));
    energy = energy * .5 + .25 * (hist(vec2(f-.018,age)) + hist(vec2(f+.018,age)));
    float valley = mix(.18, 1., smoothstep(.10, .85, abs(xz.x)));
    return energy * energy * 1.25 * uControls.x * valley;
}
void main() {
    vec2 p = aspect(vUv);
    float sway = sin(uTime * .19) * .16 * uWidth;
    vec3 ro = vec3(sway, .3 + uControls.w * .75 + uPulse * .10, -.5);
    vec3 rd = normalize(vec3(p.x, p.y - .15, .95));
    vec3 horizon = uRainbow == 1 ? hsv2rgb(vec3(.72 + .035*sin(uTime*.12),.8,1.)) : uAccent;
    vec3 col = horizon * exp(-abs(rd.y + .015)*30.) * (.15 + uTreb*.3) * uControls.z;
    col += horizon * .008;
    float distanceAlong = .12, last = 0.;
    bool hit = false;
    // Upward rays can still meet peaks above the camera. Cutting at the
    // horizon sliced every tall ridge into the same artificial flat skyline.
    if (rd.y < .35) {
        for (int i=0; i<96; ++i) {
            vec3 pos = ro + rd * distanceAlong;
            if (pos.y <= hgt(pos.xz)) { hit=true; break; }
            last=distanceAlong;
            distanceAlong += .045 + distanceAlong*.045;
            if (distanceAlong>20.) break;
        }
    }
    if (hit) {
        // Refine the crossing to avoid the old raymarch stair-step surface.
        for (int i=0;i<6;++i) {
            float m=(last+distanceAlong)*.5;
            vec3 q=ro+rd*m;
            if(q.y>hgt(q.xz))last=m;else distanceAlong=m;
        }
        vec3 pos=ro+rd*distanceAlong;
        float eps=.025, height=hgt(pos.xz);
        vec3 normal=normalize(vec3(hgt(pos.xz-vec2(eps,0.))-hgt(pos.xz+vec2(eps,0.)),2.*eps,hgt(pos.xz-vec2(0.,eps))-hgt(pos.xz+vec2(0.,eps))));
        float light=.2+.8*max(dot(normal,normalize(vec3(-.5,1.,-.4))),0.);
        vec2 g=pos.xz*mix(1.5,6.,uControls.y*.5);
        g.y -= uTime * .55;
        vec2 grid=abs(fract(g-.5)-.5);
        float footprint=max(.005,distanceAlong/uRes.y*4.);
        float line=1.-smoothstep(.006,.006+footprint,min(grid.x,grid.y));
        float hue=.58 + pos.x*.055 + height*.18;
        vec3 ink=uRainbow==1?hsv2rgb(vec3(hue,.78,1.)):uAccent;
        float crest=smoothstep(.15,.7,height);
        vec3 surface=ink*(.045+height*.14)*light;
        surface+=ink*line*(.8+uFlux*.95+crest*.45);
        surface+=mix(ink,vec3(1.),.4)*pow(crest,3.)*.18;
        col=mix(col,surface,exp(-distanceAlong*.085));
    }
    frag=vec4(col,1.);
}
