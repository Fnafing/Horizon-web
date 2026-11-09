#!/usr/bin/env python3
"""Point wasm-bindgen's bare `from "hzweb"` imports at the runtime file next to the glue."""
import sys, re
p = sys.argv[1]
s = open(p).read()
s2 = s.replace('from "hzweb"', 'from "./hzweb.js"')
open(p, 'w').write(s2)
print(p, s.count('from "hzweb"'), 'imports rewritten')
