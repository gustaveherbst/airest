#!/usr/bin/env node
/**
 * Mock MCP server for remote transport examples (HTTP + SSE).
 * Start: node examples/mcp/mcp-mock-kb-remote.mjs
 * Listens on MCP_PORT (default 3100).
 */
import http from "node:http";

const PORT = Number(process.env.MCP_PORT || 3100);
let sseResponse = null;

function mcpResult(req) {
  if (req.method === "initialize") {
    return {
      protocolVersion: "2024-11-05",
      capabilities: { tools: {} },
      serverInfo: { name: "mock-kb-remote", version: "1.0.0" },
    };
  }
  if (req.method === "tools/list") {
    return {
      tools: [
        {
          name: "search_tickets",
          description: "Search the support knowledge base for similar tickets.",
          inputSchema: {
            type: "object",
            properties: { query: { type: "string" } },
            required: ["query"],
          },
        },
      ],
    };
  }
  if (req.method === "tools/call") {
    const query = req.params?.arguments?.query ?? "";
    return {
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
  }
  return {};
}

function writeSseMessage(payload) {
  if (!sseResponse) return;
  sseResponse.write(`event: message\n`);
  sseResponse.write(`data: ${JSON.stringify(payload)}\n\n`);
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    req.on("data", (chunk) => chunks.push(chunk));
    req.on("end", () => {
      try {
        resolve(JSON.parse(Buffer.concat(chunks).toString("utf8")));
      } catch (err) {
        reject(err);
      }
    });
    req.on("error", reject);
  });
}

const server = http.createServer(async (req, res) => {
  const url = req.url?.split("?")[0] ?? "";

  if (req.method === "GET" && url === "/mcp/sse") {
    res.writeHead(200, {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache",
      Connection: "keep-alive",
    });
    sseResponse = res;
    res.write("event: endpoint\n");
    res.write("data: /mcp/message\n\n");
    req.on("close", () => {
      if (sseResponse === res) sseResponse = null;
    });
    return;
  }

  if (req.method === "POST" && (url === "/mcp" || url === "/mcp/message")) {
    let body;
    try {
      body = await readBody(req);
    } catch {
      res.writeHead(400).end();
      return;
    }

    const payload = {
      jsonrpc: "2.0",
      id: body.id,
      result: mcpResult(body),
    };

    if (url === "/mcp/message") {
      writeSseMessage(payload);
      res.writeHead(202).end();
      return;
    }

    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify(payload));
    return;
  }

  res.writeHead(404).end();
});

server.listen(PORT, () => {
  console.error(`mock MCP remote server listening on http://127.0.0.1:${PORT}`);
});
