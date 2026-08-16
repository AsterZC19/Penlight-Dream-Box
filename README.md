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

## License

MIT
