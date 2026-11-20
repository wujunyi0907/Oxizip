# External / embedded libraries

| Directory | Role | Shipped in Windows installer |
|---|---|---|
| `unrar/` | Official UnRAR CLI (RAR extract only) | Yes → `<app>\lib\unrar\` |
| `wimlib/` | Official `wimlib-imagex` + `libwim-15.dll` | Yes → `<app>\lib\wimlib\` |
| `zpaq/` | Official ZPAQ sources (`OXIZIP_EMBEDDED`) | No (compiled into oxizip binaries) |

Runtime lookup for CLIs: `OXIZIP_*` → next to the app → **`lib/<name>/`** → `PATH`.

Keep official licenses beside each redistributable. Do not modify UnRAR / wimlib binaries.
