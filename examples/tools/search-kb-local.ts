/**
 * Local in-process tool for aiREST examples (no MCP server).
 * Must export: function execute(arguments, host)
 */
function execute(args, host) {
  const query = String((args && args.query) || "");
  return {
    hits: [
      {
        id: "T-1001",
        subject: "Password reset loop",
        relevance: 0.91,
        query: query,
      },
      {
        id: "T-1002",
        subject: "Account lockout after failed logins",
        relevance: 0.84,
        query: query,
      },
    ],
  };
}
