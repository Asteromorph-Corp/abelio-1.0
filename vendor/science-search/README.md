# science-search

The `article_search` and `paper_search` tools used by the agents.

The production implementation of these tools is omitted from this release.
This crate is a stand-in backed by the [Semantic Scholar Graph API](https://api.semanticscholar.org/api-docs/graph).
The tool names, descriptions, and input schemas shown to the model are the same as in the paper runs.

| Tool             | Endpoint                                  |
| ---------------- | ----------------------------------------- |
| `article_search` | `GET /graph/v1/paper/search` (titles and abstracts) |
| `paper_search`   | `GET /graph/v1/snippet/search` (full-text snippets of open-access papers) |

Environment variables:

- `S2_API_KEY`: optional, sent as the `x-api-key` header.
  Without it the shared unauthenticated rate limit applies.
- `S2_API_URL`: optional, overrides the base URL `https://api.semanticscholar.org/graph/v1`.
