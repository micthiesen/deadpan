#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static double *x;
static size_t N;

static double value(double t) {
  double ft = floor(t), phase = t - ft;
  if (phase == 0.0) {
    long i = (long)ft;
    return i >= 0 && (size_t)i < N ? x[i] : 0.0;
  }
  double s = 0.0;
  for (size_t n = 0; n < N; ++n)
    s += ((n & 1) ? -x[n] : x[n]) / (t - (double)n);
  double f = sin(M_PI * phase) / M_PI;
  return (((long)ft & 1) ? -f : f) * s;
}

static void refine(double center, double radius, double *best_t, double *best_v) {
  double lo=center-radius, hi=center+radius;
  const double q=(sqrt(5.0)-1.0)/2.0;
  double a=hi-q*(hi-lo), b=lo+q*(hi-lo), va=fabs(value(a)), vb=fabs(value(b));
  for(int i=0;i<64;i++) {
    if(va<vb){lo=a;a=b;va=vb;b=lo+q*(hi-lo);vb=fabs(value(b));}
    else {hi=b;b=a;vb=va;a=hi-q*(hi-lo);va=fabs(value(a));}
  }
  *best_t=(lo+hi)/2; *best_v=value(*best_t);
}

int main(int argc,char **argv){
  FILE *f=fopen(argv[1],"rb"); N=argc > 2 ? strtoull(argv[2], 0, 10) : 48000; x=calloc(N,sizeof(double));
  for(size_t n=0;n<N;n++){float lr[2]; if(fread(lr,sizeof(float),2,f)!=2)return 2; x[n]=lr[0];}
  fclose(f);
  int phases=256; double bv=0,bt=0;
  for(int region=0;region<2;region++) for(int j=-4*phases;j<=4*phases;j++){
    double t=(region ? (double)N-4.0 : -4.0)+(double)(j+4*phases)/phases;
    double v=value(t); if(fabs(v)>fabs(bv)){bv=v;bt=t;}
  }
  refine(bt,1.0/phases,&bt,&bv);
  printf("frames=%zu t=%.17g peak=%.17g dBTP=%.12f sample_peak=%.17g\n",N,bt,fabs(bv),20*log10(fabs(bv)),x[1]);
  free(x);
}
