#!/usr/bin/env node
import { createInterface } from "node:readline";

const CCOS_API = "http://127.0.0.1:9023";

const rl = createInterface({ input: process.stdin });
let nextId = 1;

function respond(id, result) {
  console.log(JSON.stringify({ jsonrpc: "2.0", id, result }));
}

function respondError(id, code, message) {
  console.log(JSON.stringify({ jsonrpc: "2.0", id, error: { code, message } }));
}

const TOOLS = [
  {
    name: "ccos_memory_search",
    description: "Search CCOS causal-cognitive memory. Returns semantically relevant memory items with scores.",
    inputSchema: {
      type: "object",
      properties: {
        query: { type: "string", description: "Search query text" },
        maxResults: { type: "number", default: 8, description: "Max results to return" },
      },
      required: ["query"],
    },
  },
  {
    name: "ccos_memory_ingest",
    description: "Ingest a memory into the CCOS causal graph.",
    inputSchema: {
      type: "object",
      properties: {
        uri: { type: "string", description: "Unique identifier for this memory (e.g., tag:2026-07-05/note-about-x)" },
        source: { type: "string", description: "Content to ingest" },
      },
      required: ["uri", "source"],
    },
  },
  {
    name: "ccos_memory_stats",
    description: "Get CCOS memory graph statistics.",
    inputSchema: { type: "object", properties: {} },
  },
];

async function callCcosRecall(query, maxResults) {
  // Try hybrid first (most reliable), fall back to semantic
  for (const strategy of ["hybrid", "semantic", "task"]) {
    const resp = await fetch(`${CCOS_API}/ccos/recall`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ strategy, text: query, budget: maxResults ?? 8 }),
    });
    const data = await resp.json();
    if (data.ok && data.result?.items?.length > 0) {
      return data.result.items;
    }
  }
  return [];
}

async function callCcosIngest(uri, source) {
  const resp = await fetch(`${CCOS_API}/ccos/ingest`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ uri, source }),
  });
  const data = await resp.json();
  return data;
}

async function callCcosStats() {
  const resp = await fetch(`${CCOS_API}/ccos/stats`);
  const data = await resp.json();
  return data;
}

rl.on("line", async (line) => {
  let msg;
  try {
    msg = JSON.parse(line);
  } catch {
    return;
  }

  const id = msg.id ?? nextId++;
  const method = msg.method;

  if (method === "initialize") {
    respond(id, {
      protocolVersion: "2024-11-05",
      serverInfo: { name: "ccos-memory-bridge", version: "0.1.0" },
      capabilities: { tools: {} },
    });
    return;
  }

  if (method === "tools/list") {
    respond(id, { tools: TOOLS });
    return;
  }

  if (method === "tools/call") {
    const toolName = msg.params?.name;
    const args = msg.params?.arguments ?? {};

    try {
      let result;
      switch (toolName) {
        case "ccos_memory_search": {
          const items = await callCcosRecall(args.query, args.maxResults);
          const formatted = items.map((item) =>
            `[${item.kind}] ${item.uri} (score: ${item.score?.toFixed(3)})\n${(item.content ?? "").slice(0, 600)}`
          ).join("\n\n---\n\n");
          result = { content: [{ type: "text", text: formatted || "No results found." }] };
          break;
        }
        case "ccos_memory_ingest": {
          const report = await callCcosIngest(args.uri, args.source);
          result = { content: [{ type: "text", text: JSON.stringify(report, null, 2) }] };
          break;
        }
        case "ccos_memory_stats": {
          const stats = await callCcosStats();
          result = { content: [{ type: "text", text: JSON.stringify(stats, null, 2) }] };
          break;
        }
        default:
          respondError(id, -32601, `Tool not found: ${toolName}`);
          return;
      }
      respond(id, result);
    } catch (err) {
      respondError(id, -32603, err.message);
    }
    return;
  }

  if (method === "ping") {
    respond(id, {});
    return;
  }

  respondError(id, -32601, `Method not found: ${method}`);
});

console.error("ccos-memory-bridge MCP server ready");
