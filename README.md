# projenv

Project directory bookmark manager. Register directories with short aliases and jump to them from anywhere.

## Install

```sh
brew install gndps/tap/projenv
```

## Shell integration

Add to `~/.bash_profile` or `~/.zshrc`:

```sh
eval "$(projenv init-shell)"
```

Then use:

```sh
pa myapp    # cd to registered project
pls         # list all projects
pin alias   # register current directory as alias
prm alias   # remove a bookmark
pload name  # load a profile
```

## Commands

| Command | Description |
|---------|-------------|
| `init <alias>` | Register current directory |
| `list` | List all bookmarks; active marked with `*` |
| `activate <alias\|index>` | Print `cd /path` for eval |
| `activate git` | cd to git root |
| `activate poetry` | cd to `pyproject.toml` directory |
| `remove <alias\|index>` | Remove a bookmark |
| `profile create <name>` | Save current list as a named profile |
| `profile load <name\|index>` | Load a profile |
| `profile list` | List all profiles |
| `profile update [name]` | Update profile from current list |
| `cp <file>` | Add file to clipboard buffer |
| `paste` | Pop last buffered file as `cp <file> .` |
| `pasteall` | Print cp commands for all buffered files |

## File buffer

`projenv cp`/`paste`/`pasteall` is a lightweight FIFO for copying files across directories without leaving your terminal.
