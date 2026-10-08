// beyin.brain spike — vault [[wikilink]] → node/edge JSON (read-only on C:\beyin)
const fs = require("fs");
const path = require("path");

const VAULT = "C:/beyin";
const OUT = path.join(__dirname, "wikilink-graph.json");
const LINK_RE = /\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g;

function walk(dir, out = []) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    if (e.name.startsWith(".") || e.name === "node_modules") continue;
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, out);
    else if (e.name.endsWith(".md")) out.push(p);
  }
  return out;
}

const rel = (p) => path.relative(VAULT, p).replaceAll("\\", "/");
const stem = (p) => path.basename(p, ".md");

const files = walk(VAULT);
const byStem = new Map(); // stem → [relpath]
const byPath = new Map(); // vault-relative path w/o .md → relpath
for (const f of files) {
  const r = rel(f);
  const s = stem(f).toLowerCase();
  if (!byStem.has(s)) byStem.set(s, []);
  byStem.get(s).push(r);
  byPath.set(r.slice(0, -3).toLowerCase(), r);
}
const resolve = (target) => {
  const t = target.replace(/\/+$/, "").toLowerCase();
  if (byPath.has(t)) return [byPath.get(t)];
  // vault root is "beyin/" prefixed in some links; try stripped too
  const stripped = t.replace(/^beyin\//, "");
  if (byPath.has(stripped)) return [byPath.get(stripped)];
  return byStem.get(t.split("/").pop()) || null;
};

const edges = [];
const missing = new Set();
for (const f of files) {
  const text = fs.readFileSync(f, "utf8");
  let m;
  while ((m = LINK_RE.exec(text))) {
    const targets = resolve(m[1].trim());
    if (targets) for (const t of targets) edges.push({ from: rel(f), to: t, label: m[2] || null });
    else missing.add(m[1].trim());
  }
}

const nodes = files.map(rel);
const graph = {
  vault: VAULT,
  generated_at: new Date().toISOString(),
  node_count: nodes.length,
  edge_count: edges.length,
  unresolved: [...missing].sort(),
  nodes,
  edges,
};
fs.writeFileSync(OUT, JSON.stringify(graph, null, 1));
console.log(`${nodes.length} nodes, ${edges.length} edges, ${missing.size} unresolved → ${OUT}`);
