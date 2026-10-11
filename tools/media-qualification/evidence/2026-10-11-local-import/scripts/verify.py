from accept import *
for name in ['restored','portable']:
    # The portable package retains the exact same document and both Originals.
    run(name+'-all-pictures','verify-export',R/'Portable.deadpan','--movie',R/'output'/(name+'.mp4'),'--every','1','--samples','16016:24213,48000:56192,180000:188192','--report',R/(name+'-verification.json'))
