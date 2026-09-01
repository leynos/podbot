#!/bin/sh
set -eu
mkdir -p /root /srv/test-repos/leynos/podbot.git
cat > /root/.gitconfig <<'GITCFG'
[user]
    name = Test
    email = test@example.com
[init]
    defaultBranch = main
[url "file:///srv/test-repos/"]
    insteadOf = https://github.com/
GITCFG
git init --bare -b main /srv/test-repos/leynos/podbot.git >/dev/null 2>&1
work=$(mktemp -d)
git -C "$work" init -b main >/dev/null 2>&1
echo hello > "$work"/README.md
git -C "$work" add README.md >/dev/null 2>&1
git -C "$work" commit -m init >/dev/null 2>&1
git -C "$work" push /srv/test-repos/leynos/podbot.git main:main >/dev/null 2>&1
cat > /usr/local/bin/git-askpass <<'ASKPASS'
#!/bin/sh
echo ""
ASKPASS
chmod +x /usr/local/bin/git-askpass
echo PODBOT_E2E_READY
exec sleep infinity
