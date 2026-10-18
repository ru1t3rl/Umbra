# Rename map of ADR 0018 (D-35): the content rules, applied in this order with `sed -E -f`.
# A later rule sees the output of earlier ones. Case-sensitive. Group D (the domain term "CGF")
# has no rule: it is kept. This file and scripts/rename-check.sh are exempt from the rename (E4, E5).
# Delimiter is %, no rule contains one.

# Group A: binary and specific phrases (before the generic prefixes).
s%CARGO_BIN_EXE_cgf-cli%CARGO_BIN_EXE_umbra%g
s%#\[command\(name = "cgf-cli"%#[command(name = "umbra"%g
s%cgf-cli (run|validate|schema|demo|new-plugin|list-templates|list-plugins)\b%umbra \1%g
s%spawn `cgf-cli` as%spawn `umbra` as%g
s%sidecar packaging of `cgf-cli`%sidecar packaging of `umbra`%g
s%Engine process \(cgf-cli\)%Engine process (umbra)%g
s%cgf-server \(optional feature\)          \|%umbra-server (optional feature)        \|%g
s%cgf-engine \(facade, embeddable lib\)    \|%umbra-engine (facade, embeddable lib)  \|%g
s%example\.invalid/cgf-engine%example.invalid/umbra%g

# Group B: generic prefixes.
s%cgf-%umbra-%g
s%cgf_%umbra_%g
s%@cgf/%@umbra/%g
s%"cgf":%"umbra":%g
s%cgf\.tier%umbra.tier%g
s%CGF_%UMBRA_%g
s%Cgf([A-Z])%Umbra\1%g

# Group C: the product name (upper-case CGF that means this project).
s%The CGF Engine%The Umbra%g
s%the CGF Engine%Umbra%g
s%CGF Engine%Umbra%g
s%CGF engine%Umbra engine%g
s%CGF (template|scenario|behavior graph) file%Umbra \1 file%g
s%# CGF agent pack%# Umbra agent pack%g
s%a CGF change%an Umbra change%g
s%CGF requirements%Umbra requirements%g
s%full CGF verification%full Umbra verification%g
s%the CGF Cargo workspace%the Umbra Cargo workspace%g
s%one CGF milestone%one Umbra milestone%g
s%for the CGF \(Computer Generated Forces\) engine%for Umbra, the CGF (Computer Generated Forces) engine%g
