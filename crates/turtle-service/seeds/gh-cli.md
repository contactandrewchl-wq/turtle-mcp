# gh — GitHub CLI

Operá GitHub desde la terminal con `gh` en vez de armar URLs o manejar
tokens a mano:

- Pull requests: `gh pr create`, `gh pr checks <n>`, `gh pr merge <n> --squash`.
- Issues: `gh issue list`, `gh issue create`.
- API cruda: `gh api <endpoint>`.

`gh` ya usa la sesión autenticada del usuario; preferilo por sobre `curl`.
