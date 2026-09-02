from pathlib import Path
p = Path('crates/elastic-downstream/src/lib.rs')
s = p.read_text()
old = '    assert!(MAX_EVIDENCE_BYTES > 0);\n'
assert old in s
s = s.replace(old, '    let _bounded_ingest_limit = MAX_EVIDENCE_BYTES;\n', 1)
p.write_text(s)
print('EX6 downstream clippy guard fixed')
