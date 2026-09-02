# Penlight-Dream-Box

Garupa  API 聚合服务。

## 快速开始

```bash
cp .env.example .env
# 填写 GARUPA_UIDS、GARUPA_UUIDS、GARUPA_ENCRYPTION_KEYS、GARUPA_ENCRYPTION_IVS

docker compose up -d --build
```

验证：

```bash
curl http://127.0.0.1:8081/health
```


旧的 `GET /api/profile/export.json` 仍然保留，读取的是 Dream-API `.env`
中配置的固定玩家，方便脚本兼容；网页和上面的 POST 调用支持切换玩家。

Android 的客户端版本默认复用 `GARUPA_CLIENT_VERSIONS`，如果 Android 与
iOS 版本不同，请设置 `GARUPA_ANDROID_CLIENT_VERSIONS`。

## iOS 代理客户端获取 UID 与 UUID

脚本现在支持 Loon、Shadowrocket、Surge、Stash 和 Quantumult X，可从你自己的 Garupa 请求中捕获：

- URL `/api/user/<UID>` 中的 UID；
- 请求头 `X-Signature`，对应本项目配置里的 `GARUPA_UUIDS`。

各客户端的可导入配置和使用步骤见 [`docs/ios-credentials.md`](docs/ios-credentials.md)。所有脚本只在本机保存结果，
不修改游戏请求，也不会读取或上传加密 Key/IV。

## License

MIT
