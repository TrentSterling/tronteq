void main() {
    // Frequency increases left to right; newest analysis row is at the bottom.
    float age=clamp(vUv.y,0.,1.);
    float row=fract(uHistRow-(.5+age*127.)/128.);
    float energy=texture(uHist,vec2(vUv.x,row)).r;
    float level=pow(clamp(energy,0.,1.),mix(.7,2.3,uControls.x*.5));
    level=clamp(level*(.4+uControls.y*.6),0.,1.);
    float hue=mix(.72-level*.65,vUv.x*.78,clamp(uControls.z,0.,1.));
    vec3 ink=uRainbow==1?hsv2rgb(vec3(hue,.82,1.)):uAccent;
    vec3 col=ink*level;
    col=mix(col,vec3(1.,.92,.72),smoothstep(.86,1.,level)*.65);
    // Thin log-frequency guides and time divisions remain behind the data.
    vec2 grid=abs(fract(vec2(vUv.x*10.,vUv.y*8.)-.5)-.5);
    vec2 aa=fwidth(vec2(vUv.x*10.,vUv.y*8.));
    float lines=1.-smoothstep(.3,1.2,min(grid.x/max(aa.x,.001),grid.y/max(aa.y,.001)));
    col+=vec3(.035,.045,.065)*lines*uControls.w;
    frag=vec4(col,1.);
}
