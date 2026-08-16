#!/usr/bin/env bash
# Penlight Dream Box 裸部署脚本
# 前提：项目目录下已有编译好的两个二进制 + .env
#   /root/Penlight-Dream-Box/
#   ├─ penlight-dream-box          ← CI 编译下载
#   ├─ penlight-dream-api          ← CI 编译下载
#   ├─ .env                        ← 所有配置
#   └─ deploy/install.sh
# 用法: sudo bash deploy/install.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="${PROJECT_DIR:-$(dirname "$SCRIPT_DIR")}"
ENV_FILE="${ENV_FILE:-$PROJECT_DIR/.env}"

# GitHub Release 下载配置，可覆盖
BOX_REPO="${BOX_REPO:-AsterZC19/Penlight-Dream-Box}"
DREAM_API_REPO="${DREAM_API_REPO:-AsterZC19/Penlight-Dream-API}"
ARCH="${ARCH:-linux-x86_64}"

echo "==> 项目目录: $PROJECT_DIR"

download_release() {
  local repo="$1" asset="$2" dest="$3"
  if [ -x "$dest" ]; then
    echo "已存在 $dest，跳过下载"
    return 0
  fi
  echo "下载 $repo latest release: $asset"
  curl -fL --retry 3 -o "$dest.tmp" "https://github.com/$repo/releases/latest/download/$asset"
  chmod +x "$dest.tmp"
  mv "$dest.tmp" "$dest"
}

install_mongodb_tarball() {
  local version="${MONGO_VERSION:-7.0.17}"
  local tarball="mongodb-linux-x86_64-debian12-$version.tgz"
  local url="https://fastdl.mongodb.org/linux/$tarball"
  echo "下载 $url"
  curl -fL --retry 3 -o "/tmp/$tarball" "$url"
  tar -xzf "/tmp/$tarball" -C /opt
  local base="/opt/mongodb-linux-x86_64-debian12-$version"
  install -m 755 "$base"/bin/* /usr/local/bin/
  id -u mongodb >/dev/null 2>&1 || useradd --system --no-create-home mongodb
  mkdir -p /var/lib/mongodb /var/log/mongodb
  chown -R mongodb:mongodb /var/lib/mongodb /var/log/mongodb
  cat > /etc/systemd/system/mongod.service <<'UNIT'
[Unit]
Description=MongoDB Database Server
After=network.target

[Service]
Type=simple
User=mongodb
ExecStart=/usr/local/bin/mongod --config /etc/mongod.conf
Restart=always
RestartSec=5
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
UNIT
  systemctl daemon-reload
  echo "MongoDB $version 已通过 tarball 安装"
}

# 二进制：本地已有则复用，否则从 GitHub 下载最新 release
download_release "$BOX_REPO" "penlight-dream-box-$ARCH" "$PROJECT_DIR/penlight-dream-box"
download_release "$DREAM_API_REPO" "penlight-dream-api-$ARCH" "$PROJECT_DIR/penlight-dream-api"

if [ ! -f "$ENV_FILE" ]; then
  echo "!! 缺少 $ENV_FILE，请先创建" >&2
  exit 1
fi

echo "==> 安装 MongoDB 7"
DISTRO_ID=$(grep -oP '^ID=\K.*' /etc/os-release || true)
case "$DISTRO_ID" in
  ubuntu)
    curl -fsSL https://www.mongodb.org/static/pgp/server-7.0.asc | gpg -o /usr/share/keyrings/mongodb-server-7.0.gpg --dearmor
    echo "deb [signed-by=/usr/share/keyrings/mongodb-server-7.0.gpg] https://repo.mongodb.org/apt/ubuntu jammy/mongodb-org/7.0 multiverse" > /etc/apt/sources.list.d/mongodb-org-7.0.list
    ;;
  debian)
    curl -fsSL https://www.mongodb.org/static/pgp/server-7.0.asc | gpg -o /usr/share/keyrings/mongodb-server-7.0.gpg --dearmor
    echo "deb [signed-by=/usr/share/keyrings/mongodb-server-7.0.gpg] https://repo.mongodb.org/apt/debian bookworm/mongodb-org/7.0 main" > /etc/apt/sources.list.d/mongodb-org-7.0.list
    ;;
  *)
    echo "!! 不支持的发行版 $DISTRO_ID，将直接使用 tarball 方式安装" >&2
    ;;
esac

# Debian trixie 的 sqv 拒绝 MongoDB 源的 SHA1 绑定签名, apt 安装会失败,
# 失败时自动回退到官方 tarball 下载, 不依赖 apt 源签名。
apt-get update >/dev/null 2>&1 || true
if apt-get install -y mongodb-org >/dev/null 2>&1; then
  echo "MongoDB 已通过 apt 安装"
else
  echo "apt 安装失败，回退 tarball 方式"
  rm -f /etc/apt/sources.list.d/mongodb-org-7.0.list
  install_mongodb_tarball
fi

echo "==> 配置 MongoDB 内存限制"
install -m 644 "$SCRIPT_DIR/mongod.conf" /etc/mongod.conf
systemctl enable --now mongod

echo "==> 安装二进制"
install -m 755 "$PROJECT_DIR/penlight-dream-box" /usr/local/bin/
install -m 755 "$PROJECT_DIR/penlight-dream-api" /usr/local/bin/

echo "==> 创建运行用户"
id -u box >/dev/null 2>&1 || useradd --system --no-create-home box
chown box:box "$ENV_FILE"
chmod 600 "$ENV_FILE"

echo "==> 注册 systemd 服务"
for unit in penlight-dream-box penlight-dream-api; do
  sed "s|@ENV_FILE@|$ENV_FILE|g" "$SCRIPT_DIR/$unit.service" > "/etc/systemd/system/$unit.service"
done
systemctl daemon-reload
systemctl enable --now penlight-dream-api
systemctl enable --now penlight-dream-box

echo "==> 完成"
echo "  编辑配置: vim $ENV_FILE && systemctl restart penlight-dream-api penlight-dream-box"
echo "  验证:     curl http://127.0.0.1:8081/health"
echo "  日志:     journalctl -u penlight-dream-box -f"
