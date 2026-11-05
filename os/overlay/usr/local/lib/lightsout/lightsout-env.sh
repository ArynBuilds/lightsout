# Environment for everything in Lights Out. Home is your Windows user folder
# (see lightsout-home), so CLIs share settings, logins and sessions with Windows.

# Programs and caches stay Linux-only (per-system folders, see per-system.conf)
case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) export PATH="$HOME/.local/bin:$PATH" ;; esac
export npm_config_prefix="$HOME/.local"        # `npm install -g` without sudo
export XDG_CACHE_HOME="$HOME/.cache"

# git: your Windows settings (name, email, aliases), but logins through gh here
# (Windows' credential manager doesn't exist on Linux)
export GIT_CONFIG_COUNT=2
export GIT_CONFIG_KEY_0=credential.helper GIT_CONFIG_VALUE_0=
export GIT_CONFIG_KEY_1=credential.helper GIT_CONFIG_VALUE_1='!gh auth git-credential'

alias c='cd /mnt/c'
# Windows paths work too: cd c:/dev/app, cd 'C:\DEV\app' (quoted: bash drops a bare \)
cd() {
  local win='^([A-Za-z]):[\/]?(.*)$'
  if [[ ${1:-} =~ $win ]]; then set -- "/mnt/${BASH_REMATCH[1],,}/${BASH_REMATCH[2]//\\//}"; fi
  builtin cd "$@"
}

# First start: a hint when no agent is installed yet (installs stay on disk)
if [ -n "${PS1:-}" ] && ! command -v claude >/dev/null 2>&1 && ! command -v codex >/dev/null 2>&1; then
  echo "Lights Out: no agent installed yet. Run  lightsout-install-agents  (or install any CLI the usual way)."
fi
