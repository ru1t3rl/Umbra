# AffiliationTag

An AffiliationTag shows a force's APP-6D affiliation in a list or card, using the standard frame shape (rectangle friend, diamond hostile, square neutral, quatrefoil unknown) and the standard fill (`affil-friend`, `affil-hostile`, `affil-neutral`, `affil-unknown`) framed by `symbol-frame`.

**You provide** `affiliation` and, optionally, a short code as children (echelon or formation: "BN", "2 MECH BDE") replacing the default word.

**Do**: use it wherever an entity appears outside the map, so the list and the map agree; keep the text in the mono `data-sm` style the component applies.

**Don't**: colour the text with an affiliation fill (text stays `ink`); use affiliation colours for anything that is not a force; invent a fifth affiliation colour, APP-6D also defines assumed friend, suspect and pending, use the base four unless the scenario requires them.

20px symbol, 1.5px frame, `surface-2` ground, `radius-xs`. In the Day theme the standard fills depend on the frame for visibility by design.
