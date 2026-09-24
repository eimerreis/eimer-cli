import { expect, test } from "bun:test";
import { AzureDevOpsClient } from "./azure-client";

test("an Azure DevOps sign-in redirect reports rejected authentication", async () => {
  const server = Bun.serve({
    port: 0,
    fetch(request) {
      if (new URL(request.url).pathname === "/signin") {
        return new Response("<html><title>Azure DevOps Services | Sign In</title></html>", {
          status: 203,
          headers: { "content-type": "text/html; charset=utf-8" },
        });
      }

      return Response.redirect(new URL("/signin", request.url), 302);
    },
  });

  try {
    const baseUrl = `http://localhost:${server.port}`;
    const client = new AzureDevOpsClient(
      { organizationUrl: baseUrl, project: "test", baseUrl },
      { scheme: "bearer", token: "invalid" },
    );

    await expect(client.getJson("/_apis/build/definitions")).rejects.toThrow(
      /Azure DevOps returned a sign-in page.*authentication was rejected/i,
    );
  } finally {
    server.stop(true);
  }
});
