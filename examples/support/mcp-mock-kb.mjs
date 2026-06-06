#!/usr/bin/env node
/**
 * Minimal MCP stdio server for aiREST examples (tools/list + tools/call).
 */
import readline from "node:readline";

const rl = readline.createInterface({ input: process.stdin, output: process.stdout, terminal: false });

rl.on("line", (line) => {
  let req;
  try {
    req = JSON.parse(line);
  } catch {
    return;
  }

  let result;
  if (req.method === "initialize") {
    result = {
      protocolVersion: "2024-11-05",
      capabilities: { tools: {} },
      serverInfo: { name: "mock-kb", version: "1.0.0" },
    };
  } else if (req.method === "tools/list") {
    result = {
      tools: [
        {
          name: "search_tickets",
          description: "Search the support knowledge base for similar tickets.",
          inputSchema: {
            type: "object",
            properties: {
              query: { type: "string", description: "Search query" },
            },
            required: ["query"],
          },
        },
      ],
    };
  } else if (req.method === "tools/call") {
    const query = req.params?.arguments?.query ?? "";
    result = {
      content: [
        {
          type: "text",
          text: JSON.stringify({
            hits: [
              {
                id: "T-1001",
                subject: "Password reset loop",
                relevance: 0.91,
                query,
              },
            ],
          }),
        },
      ],
    };
  } else {
    result = {};
  }

  process.stdout.write(
    JSON.stringify({ jsonrpc: "2.0", id: req.id, result }) + "\n"
  );
});
