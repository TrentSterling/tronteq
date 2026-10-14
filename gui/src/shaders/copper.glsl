// Thin layered raster ribbons; spectral height and beat accents never sum to white.
void main() {
    vec3 color=vec3(0.);
    float count=4.+floor(uControls.x*4.);
    for(int i=0;i<12;i++) {
        float n=float(i);if(n>=count)break;
        float band=spec((n+.5)/count);
        float phase=uTime*(.32+fract(n*.618)*.48)+n*2.39996;
        float center=.5+sin(phase)*.30+sin(phase*1.7+n)*.07;
        center+=sin(vUv.x*5.+phase)*(.008+.015*uControls.z)*uMid;
        center+=sin(uPhase*6.2831853)*uPulse*.024;
        float width=(.004+.011*(uControls.y*.5))*(.7+band*.65);
        float d=vUv.y-center;
        float body=exp(-d*d/(width*width));
        float halo=exp(-abs(d)/max(width*2.5,.002))*.10;
        float metallic=.45+.55*smoothstep(-width,width,d);
        vec3 tint=uRainbow==1?hsv2rgb(vec3(fract(n/count+uTime*.018),.85,1.)):uAccent;
        vec3 ribbon=tint*(body*metallic+halo)*(.35+band*.55+uPulse*.18);
        ribbon+=mix(tint,vec3(1.),.3)*exp(-pow((d-width*.4)/(width*.17),2.))*.18;
        color=max(color,ribbon);
    }
    float scan=.95+.05*sin(vUv.y*uRes.y*1.2+uTime*4.)*uTreb;
    vec3 trail=texture(uPrev,vUv).rgb*mix(.3,.90,uControls.w*.5);
    frag=vec4(max(color*scan,trail),1.);
}
