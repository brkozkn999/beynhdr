#!/usr/bin/env node
/*
 * lane08 round-trip verifier for beyn.layouts presets.
 *
 * Per preset:
 *   1. layout.apply preset -> tab A (focused)
 *   2. pane.layout (geometry) + layout.export on tab A
 *   3. layout.apply exported root -> fresh tab B
 *   4. pane.layout + layout.export on tab B
 *   5. assert normalized trees equal (eps on ratio) and rect multisets equal
 *   6. close tab B
 *
 * Usage: node verify.js <preset.json> [--keep]
 */
"use strict";

const fs = require("node:fs");
const net = require("node:net");
const os = require("node:os");
const path = require("node:path");

const argv = process.argv.slice(2);
const PRESET = argv[0];
const KEEP = argv.includes("--keep");

const SOCK = path.join(
  process.env.APPDATA || path.join(os.homedir(), "AppData", "Roaming"),
  "herdr",
  "sessions",
  process.env.HERDR_SESSION || "ln08",
  "herdr.sock",
);
const PIPE = "\\\\.\\pipe\\" + SOCK;

let seq = 0;
function request(method, params) {
  return new Promise((resolve, reject) => {
    const payload = JSON.stringify({ id: `v${++seq}`, method, params });
    const conn = net.connect(PIPE, () => conn.write(payload + "\n"));
    let buf = "";
    conn.setEncoding("utf8");
    conn.on("data", (chunk) => {
      buf += chunk;
      const nl = buf.indexOf("\n");
      if (nl >= 0) {
        conn.end();
        try {
          resolve(JSON.parse(buf.slice(0, nl)));
        } catch (e) {
          reject(e);
        }
      }
    });
    conn.on("error", reject);
  });
}

const EPS = 1e-5;

// Strip instance data; keep structural + semantic fields.
function normNode(n) {
  if (n.type === "pane") {
    return {
      type: "pane",
      label: n.label ?? null,
      has_command: n.command != null,
    };
  }
  return {
    type: "split",
    direction: n.direction,
    ratio: n.ratio,
    first: normNode(n.first),
    second: normNode(n.second),
  };
}

function treesEqual(a, b, diffs, trail = "root") {
  if (a.type !== b.type) {
    diffs.push(`${trail}: type ${a.type} != ${b.type}`);
    return;
  }
  if (a.type === "pane") {
    if ((a.label ?? null) !== (b.label ?? null))
      diffs.push(`${trail}: label ${a.label} != ${b.label}`);
    return;
  }
  if (a.direction !== b.direction)
    diffs.push(`${trail}: direction ${a.direction} != ${b.direction}`);
  if (Math.abs(a.ratio - b.ratio) > EPS)
    diffs.push(`${trail}: ratio ${a.ratio} != ${b.ratio}`);
  treesEqual(a.first, b.first, diffs, `${trail}.first`);
  treesEqual(a.second, b.second, diffs, `${trail}.second`);
}

function rectKey(r) {
  return `${r.x},${r.y},${r.width},${r.height}`;
}

function rects(snapshot) {
  return snapshot.panes.map((p) => rectKey(p.rect)).sort();
}

function firstPaneId(node) {
  return node.type === "pane" ? node.pane_id : firstPaneId(node.first);
}

async function main() {
  const preset = JSON.parse(fs.readFileSync(PRESET, "utf8"));
  const name = preset.preset || path.basename(PRESET, ".json");
  const out = { preset: name, steps: {}, verdict: "FAIL", diffs: [] };

  // 1. apply preset
  let resp = await request("layout.apply", {
    root: preset.root,
    tab_label: preset.tab_label,
    focus: true,
  });
  if (resp.error) {
    out.steps.apply = resp.error;
    console.log(JSON.stringify(out, null, 2));
    process.exit(1);
  }
  const tabA = resp.result.layout.tab_id;
  const exportA = resp.result.layout;
  out.steps.apply = { tab_id: tabA, focused_pane_id: exportA.focused_pane_id };

  // 2. geometry + export on A
  resp = await request("pane.layout", { pane_id: firstPaneId(exportA.root) });
  const geomA = resp.result.layout;
  out.steps.geomA = {
    tab_id: geomA.tab_id,
    area: geomA.area,
    panes: geomA.panes.map((p) => p.rect),
    splits: geomA.splits.map((s) => ({
      id: s.id,
      direction: s.direction,
      ratio: s.ratio,
    })),
  };
  resp = await request("layout.export", { tab_id: tabA });
  const expA = resp.result.layout;
  out.steps.exportA = { tab_id: expA.tab_id, root: expA.root };

  // 3. apply exported root -> tab B (the round trip)
  resp = await request("layout.apply", {
    root: expA.root,
    tab_label: `${preset.tab_label || name}-rt`,
    focus: true,
  });
  if (resp.error) {
    out.steps.applyB = resp.error;
    console.log(JSON.stringify(out, null, 2));
    process.exit(1);
  }
  const tabB = resp.result.layout.tab_id;
  const exportB = resp.result.layout;
  out.steps.applyB = { tab_id: tabB };

  // 4. geometry + export on B
  resp = await request("pane.layout", { pane_id: firstPaneId(exportB.root) });
  const geomB = resp.result.layout;
  out.steps.geomB = {
    tab_id: geomB.tab_id,
    panes: geomB.panes.map((p) => p.rect),
    splits: geomB.splits.map((s) => ({
      id: s.id,
      direction: s.direction,
      ratio: s.ratio,
    })),
  };
  resp = await request("layout.export", { tab_id: tabB });
  const expB = resp.result.layout;

  // 5. compare
  const diffs = [];
  treesEqual(normNode(expA.root), normNode(expB.root), diffs);
  const ra = rects(geomA);
  const rb = rects(geomB);
  if (JSON.stringify(ra) !== JSON.stringify(rb)) {
    diffs.push(`rects differ:\n  A=${JSON.stringify(ra)}\n  B=${JSON.stringify(rb)}`);
  }
  // ratio-level comparison of the exported trees (raw, not normed)
  const rawDiffs = [];
  treesEqual(
    stripIds(expA.root),
    stripIds(expB.root),
    rawDiffs,
  );
  out.diffs = diffs.concat(rawDiffs.map((d) => `raw: ${d}`));
  out.verdict = out.diffs.length === 0 ? "PASS" : "FAIL";

  // 6. cleanup: close the round-trip tab B unless --keep
  if (!KEEP) {
    resp = await request("tab.close", { tab_id: tabB });
    out.steps.closedB = resp.error ? resp.error : { closed: tabB };
  }

  console.log(JSON.stringify(out, null, 2));
  process.exit(out.verdict === "PASS" ? 0 : 1);
}

function stripIds(n) {
  if (n.type === "pane") {
    const { pane_id, cwd, env, ...rest } = n;
    return rest;
  }
  return {
    type: "split",
    direction: n.direction,
    ratio: n.ratio,
    first: stripIds(n.first),
    second: stripIds(n.second),
  };
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
