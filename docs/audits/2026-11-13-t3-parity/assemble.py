"""Validate the audit records and reproduce its consolidated inventory."""

import argparse
import csv
import io
import json
import re
from collections import Counter
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("--write", action="store_true")
parser.add_argument("--t3", type=Path, default=Path("/tmp/ferrite-t3-audit-source-20261113"))
args = parser.parse_args()
audit = Path(__file__).resolve().parent
repo = audit.parents[2]
sources = {
    "ferrite_evidence": (repo, "josephwylie/ferrite", "62947f91d5cd6ee494aaae9ed09d794a853e5bab"),
    "t3_evidence": (args.t3, "pingdotgg/t3code", "74573fc29b901bafbf8f1dbb2eb29606da1fa448"),
}
required = {"id", "title", "priority", "category", "evidence_level", "impact", "ferrite_evidence", "t3_evidence", "recommendation", "acceptance"}
expected = {"pr-navigation", "pr-operations", "subagents-background", "browser", "input-focus", "motion", "panel-tools", "composer", "transcript", "providers-settings", "projects-navigation", "coverage-quality", "link-affordances", "scroll-quality", "quote-attachments", "reconciliation", "quality-correction"}
categories = {"defect", "missing-capability", "partial-capability", "interaction", "motion", "intentional-difference", "verification-gap"}
levels = {"runtime-reproduced", "source-proven", "inferred", "unverified"}
review = json.loads((audit / "adjudications.json").read_text())
records = {}
seen_slices = set()
references = set()
unparsed = []
for path in sorted((audit / "reports").glob("*.json")):
    if path.stem not in expected:
        continue
    seen_slices.add(path.stem)
    issues = json.loads(path.read_text())
    assert isinstance(issues, list), path
    for original in issues:
        assert required <= original.keys(), (path, required - original.keys())
        item = dict(original, slice=path.stem, report=f"reports/{path.stem}.md")
        assert item["id"] not in records, item["id"]
        item.update(review.get("overrides", {}).get(item["id"], {}))
        assert item["priority"] in {"P0", "P1", "P2", "P3"}, item
        assert item["category"] in categories and item["evidence_level"] in levels, item
        for evidence in item.get("runtime_evidence", []):
            assert (audit / evidence).is_file(), (item["id"], evidence)
        for field, (root, remote, commit) in sources.items():
            links = []
            for citation in item[field]:
                assert isinstance(citation, str), (item["id"], field)
                url = re.search(r"https://github.com/[^/]+/[^/]+/blob/([^/]+)/([^#]+)#L(\d+)(?:-L(\d+))?", citation)
                bare = re.search(r"((?:crates|vendor|apps|packages|scripts|docs|\.github)/[^\s:#]+):(\d+)(?:-(\d+))?", citation)
                if url:
                    sha, filename, start, end = url.groups()
                    assert sha == commit, (item["id"], citation)
                elif bare:
                    filename, start, end = bare.groups()
                else:
                    unparsed.append({"id": item["id"], "field": field, "citation": citation})
                    links.append(citation)
                    continue
                target = root / filename
                assert target.is_file(), (item["id"], target)
                lines = len(target.read_text(errors="replace").splitlines())
                assert 1 <= int(start) <= int(end or start) <= lines, (item["id"], citation, lines)
                references.add((field, filename, start, end))
                suffix = f"#L{start}" + (f"-L{end}" if end else "")
                links.append(f"https://github.com/{remote}/blob/{commit}/{filename}{suffix}")
            item[field] = list(dict.fromkeys(links))
        if item["category"] == "verification-gap":
            item.setdefault("disposition", "verification-gap")
        elif item["category"] == "intentional-difference":
            item.setdefault("disposition", "product-choice")
        elif item["category"] == "missing-capability":
            item.setdefault("disposition", "capability-candidate")
        else:
            item.setdefault("disposition", "repair-or-quality")
        records[item["id"]] = item
assert seen_slices == expected, ("Missing partitions", expected - seen_slices)
assert set(review.get("overrides", {})) <= records.keys(), "Unknown override"
merged = review.get("merge", {})
for duplicate, canonical in merged.items():
    assert duplicate in records and canonical in records and canonical not in merged
    target, item = records[canonical], records[duplicate]
    target.setdefault("related_records", []).append({"id": duplicate, "title": item["title"], "report": item["report"], "impact": item["impact"], "acceptance": item["acceptance"]})
    for field in sources:
        target[field] = list(dict.fromkeys(target[field] + item[field]))
issues = sorted((item for key, item in records.items() if key not in merged), key=lambda item: (item["priority"], item["slice"], item["id"]))
counts = {
    "raw_records": len(records), "consolidated_records": len(issues), "merged_records": len(merged),
    "priorities": dict(sorted(Counter(i["priority"] for i in issues).items())),
    "categories": dict(sorted(Counter(i["category"] for i in issues).items())),
    "evidence_levels": dict(sorted(Counter(i["evidence_level"] for i in issues).items())),
    "dispositions": dict(sorted(Counter(i["disposition"] for i in issues).items())),
    "slices": dict(sorted(Counter(i["slice"] for i in issues).items())),
    "validated_source_references": len(references), "unparsed_evidence": unparsed,
}
markdown = ["# Consolidated finding inventory", "", "The user wants a production-quality audit of Ferrite against T3 Code. This inventory records complete behavior gaps, not just feature presence. Priorities and merges follow [adjudications.json](adjudications.json). Source proof is not a claim of runtime reproduction. Capability candidates require a product decision and are not automatically Ferrite defects.", "", f"There are {len(issues)} consolidated records from {len(records)} original records across {len(expected)} investigation partitions.", "", "| ID | Priority | Evidence | Category | Finding |", "|---|---|---|---|---|"]
for item in issues:
    title = item["title"].replace("|", "\\|")
    markdown.append(f"| [{item['id']}](#{item['id'].lower()}) | {item['priority']} | {item['evidence_level']} | {item['category']} | {title} |")
for item in issues:
    markdown.extend(["", f"## {item['id']}", "", item["title"], "", f"{item['priority']}. {item['category']}. {item['evidence_level']}. Disposition `{item['disposition']}`.", "", item["impact"], "", f"Recommendation. {item['recommendation']}", "", f"Acceptance check. {item['acceptance']}", "", f"[Full investigation and reproduction conditions]({item['report']}).", "", "Evidence:", ""])
    for field in sources:
        for citation in item[field]:
            label = citation.split("/blob/")[-1].split("/", 1)[-1].replace("#L", ":").replace("-L", "-")
            markdown.append(f"- [{label}]({citation})" if citation.startswith("https://") else f"- {citation}")
    for related in item.get("related_records", []):
        markdown.extend(["", f"Related record [{related['id']}]({related['report']}). {related['impact']}", "", f"Additional acceptance check. {related['acceptance']}"])
    if item.get("review_note"):
        markdown.extend(["", "Review note. " + item["review_note"]])
    if item.get("runtime_evidence"):
        markdown.extend(["", "Runtime evidence:", ""])
        for evidence in item["runtime_evidence"]:
            markdown.append(f"- [{Path(evidence).name}]({evidence})")
csv_buffer = io.StringIO()
writer = csv.DictWriter(csv_buffer, fieldnames=["id", "priority", "category", "evidence_level", "disposition", "slice", "title", "impact", "recommendation", "acceptance", "report", "review_note", "runtime_evidence"], lineterminator="\n")
writer.writeheader()
for item in issues:
    row = {key: item.get(key, "") for key in writer.fieldnames}
    row["runtime_evidence"] = "; ".join(item.get("runtime_evidence", []))
    writer.writerow(row)
outputs = {"issues.json": json.dumps(issues, indent=2) + "\n", "counts.json": json.dumps(counts, indent=2) + "\n", "findings.md": "\n".join(markdown) + "\n", "tally.csv": csv_buffer.getvalue()}
for filename, content in outputs.items():
    target = audit / filename
    if args.write:
        target.write_text(content)
    else:
        assert target.read_text() == content, f"Regenerate {filename} with --write"
print(json.dumps(counts, indent=2))
