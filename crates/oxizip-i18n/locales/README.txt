Oxizip language packs (locales)
================================

These are plain TOML files. After install you can:

  • Edit zh-CN.toml / en.toml to change wording
  • Copy en.toml to e.g. ja.toml, set code/cli/native, translate [messages]

Then restart Oxizip — new languages appear under Settings → Language.

Search order (later overrides the same `code`):
  1) Built-in defaults
  2) This folder next to oxizip.exe  (or /usr/share/oxizip/locales on Linux)
  3) User folder:
       Windows: %APPDATA%\Oxizip\locales\
       Linux:   ~/.config/oxizip/locales/
  4) OXIZIP_LOCALES environment variable (optional)

Minimal file shape:

  code = "ja"
  cli = "ja"
  native = "日本語"

  [messages]
  "cli.done" = "完了"
  "app.name" = "Oxizip"
