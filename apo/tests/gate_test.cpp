// gate_test.cpp: portable check of the mic-stack gate DSP (no Windows headers).
// From apo/: g++ -std=c++17 -O2 -Isrc tests/gate_test.cpp src/Gate.cpp -o build/gate_test
//   or cl /std:c++17 /EHsc /Isrc tests\gate_test.cpp src\Gate.cpp /Fe:build\gate_test.exe
#include "Gate.h"
#include <cmath>
#include <cstdio>
#include <vector>
using namespace tronteq;
static float rms(const std::vector<float>& v, size_t a, size_t b){double s=0;for(size_t i=a;i<b;i++)s+=v[i]*v[i];return std::sqrt(s/(b-a));}
int main(){
  const double fs=48000; GateProcessor g; g.Reset(fs);
  Gate p{1,-50.f,6.f,-24.f,2.f,200.f,250.f,0};
  p=SanitizeGate(p);
  // 1 s noise floor at -70 dBFS, then 1 s speech-ish tone at -20 dBFS, then 1 s noise floor (stereo).
  const size_t n=48000; std::vector<float> buf(n*3*2);
  unsigned s=1; auto rnd=[&]{s=s*1103515245+12345;return ((s>>16)&0x7fff)/16384.0f-1.0f;};
  for(size_t i=0;i<n*3;i++){ float x; if(i>=n&&i<2*n) x=0.1f*std::sin(2*3.14159265358979323846*220*i/fs); else x=0.000316f*rnd(); buf[2*i]=x; buf[2*i+1]=x;}
  std::vector<float> in=buf;
  for(size_t off=0; off<n*3; off+=480) g.Process(&buf[off*2],480,2,p);
  auto gain=[&](size_t a,size_t b){return 20*std::log10(rms(buf,a*2,b*2)/rms(in,a*2,b*2));};
  double floor1=gain(n/2,n), speech=gain(n+2400,2*n), holdTail=gain(2*n,2*n+4800), floor2=gain(2*n+n/2,3*n);
  printf("floor before %.1f dB, speech %.2f dB, first 100ms after %.1f dB, floor after %.1f dB\n",floor1,speech,holdTail,floor2);
  bool ok = floor1<-23 && floor1>-25 && speech>-0.1 && holdTail>-1.0 && floor2<-23;
  // NaN params sanitize to defaults; disabled is unity
  Gate bad{1,NAN,NAN,NAN,NAN,NAN,NAN,7}; Gate sb=SanitizeGate(bad);
  ok = ok && sb.threshold_db==-50.f && sb.range_db==-24.f && sb._reserved==0;
  std::vector<float> t(960,0.001f); Gate off=p; off.enabled=0; g.Process(t.data(),480,2,off); ok = ok && t[0]==0.001f && g.GainDb()==0.0;
  printf(ok?"PASS\n":"FAIL\n"); return ok?0:1;
}
