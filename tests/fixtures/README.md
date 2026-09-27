# Portable schema fixtures

`schema-v1-cabinet.pmcab` is a frozen version-1 document, independent of the
current serializer and installed catalog. Keep it version 1 as the schema evolves.
It contains exact micrometre dimensions/trims/placements, BRL prices and fees,
effective board thickness/grain differing from material defaults, rotated parent
and child poses with sub-micrometre translations, locked allocation, and a pinned
FGVTN catalog snapshot including its real source SHA-256 and attribution.

The historical receipt's manufacturing hashes came from the existing `wood-v1` /
`packet-v2` fingerprint computation. Its file hash came from an actual draft PDF
written and read back through `write_pdf`; the PDF is not needed to open the
project and is deliberately not bundled. The receipt path and completion time
were normalized to fixed fixture values. They are historical test data, not a
claim that an export is available on the receiving computer. Migration must retain
them exactly and must not fetch that path or the catalog URL.

Portability tests compare the entire migrated payload (only `schema_version`
changes), not just object counts, and exercise explicit schema-2 saves. The legacy
optional-field variant is formed by removing only documented optional fields
from this frozen input; it never uses the current serializer to construct v1 data.
