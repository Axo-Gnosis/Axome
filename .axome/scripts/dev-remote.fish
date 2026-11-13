#!/usr/bin/env fish

# Dev desktop + local daemon + live web frontend, sandboxed under
# .axome/dev-remote and trusting each other up front, so no pairing step and
# no ~/.config remotes. `--web-only` skips the desktop app.

set -l repo_root (realpath (status dirname)/../..)
cd $repo_root

set -l root $repo_root/.axome/dev-remote
set -l server_config_dir $root/config
set -l server_data_dir $root/data
set -l server_cache_dir $root/cache
# The desktop reads settings and client.pem from $XDG_CONFIG_HOME/axome and
# keeps its DB under $XDG_DATA_HOME and caches under $XDG_CACHE_HOME; child
# processes never inherit them.
set -l desktop_config_home $root/desktop/config
set -l desktop_data_home $root/desktop/data
set -l desktop_cache_home $root/desktop/cache
set -l client_key $desktop_config_home/axome/client.pem
set -l desktop_settings $desktop_config_home/axome/settings.jsonc

set -q AXOME_DEV_PORT; or set -l AXOME_DEV_PORT 7430
set -q AXOME_DEV_WEB_PORT; or set -l AXOME_DEV_WEB_PORT 7431

mkdir -p $server_config_dir $server_data_dir $desktop_config_home/axome $desktop_data_home $root/web

# Vite serves the live web UI on :1430 and its sockets reach the daemon's web port
# directly (trusted via web.allowedOrigins), so the daemon's own web page only
# forwards there; no bundle to rebuild or go stale.
echo '<!doctype html><script>location.replace("http://127.0.0.1:1430" + location.pathname + location.search)</script>' >$root/web/index.html

cargo build -p axome-server; or exit 1
set -l server ./target/debug/axome-server

if not test -f $client_key
    $server keygen $client_key >/dev/null; or exit 1
end
$server fingerprint $client_key >$root/authorized_keys; or exit 1

printf '{
  "authorizedKeysFile": "%s",
  "web": {
    "listen": "127.0.0.1:%s",
    "assetsDir": "%s",
    "allowedOrigins": ["http://127.0.0.1:1430"]
  }
}\n' $root/authorized_keys $AXOME_DEV_WEB_PORT $root/web >$server_config_dir/server.jsonc

set -l server_fingerprint ($server --config-dir $server_config_dir fingerprint); or exit 1
if not test -f $desktop_settings
    printf '{
  "remotes": { "dev": { "address": "127.0.0.1:%s", "fingerprint": "%s" } }
}\n' $AXOME_DEV_PORT $server_fingerprint >$desktop_settings
else
    bun -e '
      // An ES import, not require(): under CommonJS `bun -e` exits 0 on an uncaught throw.
      import fs from "node:fs";
      const [path, port, fp] = process.argv.slice(1);
      const text = fs.readFileSync(path, "utf-8");
      const settings = Bun.JSONC.parse(text);
      const dev = { address: "127.0.0.1:" + port, fingerprint: fp };
      const current = settings.remotes?.dev;
      if (current?.address === dev.address && current?.fingerprint === dev.fingerprint) process.exit(0);
      try { JSON.parse(text); } catch {
        console.error(`dev-remote: set remotes.dev to ${JSON.stringify(dev)} in ${path}; it has comments a rewrite would drop`);
        process.exit(1);
      }
      settings.remotes = { ...settings.remotes, dev };
      fs.writeFileSync(path, JSON.stringify(settings, null, 2) + "\n");
    ' $desktop_settings $AXOME_DEV_PORT $server_fingerprint; or exit 1
end

setpriv --pdeathsig TERM $server \
    --config-dir $server_config_dir \
    --data-dir $server_data_dir \
    --cache-dir $server_cache_dir \
    serve --listen 127.0.0.1:$AXOME_DEV_PORT &
set -l server_pid $last_pid

sleep 0.2
if not kill -0 $server_pid 2>/dev/null
    echo "axome-server failed to start" >&2
    exit 1
end

# Through bun: the devshell has no node for vite's shebang, and running the
# file in-process keeps --pdeathsig on vite itself.
VITE_AXOME_ORIGIN="http://127.0.0.1:$AXOME_DEV_WEB_PORT" AXOME_TARGET=web setpriv --pdeathsig TERM bun ./node_modules/.bin/vite dev &
set -l vite_pid $last_pid

if contains -- --web-only $argv
    while kill -0 $server_pid 2>/dev/null; and kill -0 $vite_pid 2>/dev/null
        sleep 1
    end
    kill $server_pid $vite_pid 2>/dev/null
    wait
else
    # exec hands this PID to the app, so closing it fires --pdeathsig on the daemon and vite.
    XDG_CONFIG_HOME=$desktop_config_home XDG_DATA_HOME=$desktop_data_home XDG_CACHE_HOME=$desktop_cache_home exec bun app:dev
end
