# mrcr

AI code review for your uncommitted git changes, from the command line. This package is a small launcher
for the [MrCodeReview](https://github.com/MR-TABATA/MrCodeReview) `mrcr` CLI.

- **macOS only** for now. The binary is universal (Apple Silicon and Intel); it has been tested on Apple Silicon only.
  On other systems it prints a message and exits with code 1.
- The launcher has no install scripts. The binary comes from the `@mr-tabata/mrcr-darwin` package on npm.

```sh
npx mrcr@<version> --help
```

## Use as an MCP server

Pin the version. Without a version, `npx` runs the latest code on every start.

```json
{
  "mcpServers": {
    "mrcodereview": {
      "command": "npx",
      "args": ["-y", "mrcr@<version>", "mcp"]
    }
  }
}
```

`npx mrcr@<version> mcp setup` writes this entry for you (agy), and prints the commands for Claude Code and VS Code.
Update `<version>` yourself when you upgrade; the review database is upgraded by newer versions, and an older pinned
version refuses to open a newer database ("update this app").

## More

Documentation, the desktop app and the issue tracker: <https://github.com/MR-TABATA/MrCodeReview>. MIT license.
