(() => {
  "use strict";

  const config = window.PENLIGHT_CONFIG || {};
  const apiPrefix = String(config.apiPrefix || "/api").replace(/\/+$/, "");
  const endpoint = apiPrefix + "/profile/export.json";

  const form = document.querySelector("#export-form");
  const uidInput = document.querySelector("#uid");
  const uuidInput = document.querySelector("#uuid");
  const platformInput = document.querySelector("#platform");
  const apiKeyInput = document.querySelector("#api-key");
  const downloadButton = document.querySelector("#download-button");
  const formStatus = document.querySelector("#form-status");
  const credentialJson = document.querySelector("#credential-json");
  const jsonStatus = document.querySelector("#json-status");

  function setStatus(element, message, kind = "") {
    element.textContent = message;
    element.className = element.className
      .replace(/\s+is-(error|success)/g, "")
      .trim();
    if (kind) element.classList.add("is-" + kind);
  }

  function setPlatform(platform) {
    platformInput.value = platform;
    document.querySelectorAll("[data-platform]").forEach((button) => {
      button.classList.toggle("is-selected", button.dataset.platform === platform);
    });
  }

  document.querySelectorAll("[data-platform]").forEach((button) => {
    button.addEventListener("click", () => setPlatform(button.dataset.platform));
  });

  document.querySelector("#toggle-uuid").addEventListener("click", (event) => {
    const visible = uuidInput.type === "text";
    uuidInput.type = visible ? "password" : "text";
    event.currentTarget.textContent = visible ? "显示" : "隐藏";
    event.currentTarget.setAttribute("aria-label", visible ? "显示 UUID" : "隐藏 UUID");
  });

  function scalar(value) {
    if (typeof value === "number" && Number.isFinite(value)) return String(value);
    if (typeof value === "string") return value.trim();
    return "";
  }

  function findValue(root, names, depth = 0) {
    if (depth > 5 || root === null || root === undefined) return "";
    if (Array.isArray(root)) {
      for (const item of root) {
        const result = findValue(item, names, depth + 1);
        if (result) return result;
      }
      return "";
    }
    if (typeof root !== "object") return "";

    for (const [key, value] of Object.entries(root)) {
      if (names.includes(key.toLowerCase())) {
        const result = scalar(value);
        if (result) return result;
      }
    }
    for (const value of Object.values(root)) {
      const result = findValue(value, names, depth + 1);
      if (result) return result;
    }
    return "";
  }

  document.querySelector("#parse-json").addEventListener("click", () => {
    const raw = credentialJson.value.trim();
    if (!raw) {
      setStatus(jsonStatus, "请先粘贴 Loon 导出的 JSON。", "error");
      return;
    }

    let parsed;
    try {
      parsed = JSON.parse(raw);
    } catch (_) {
      setStatus(jsonStatus, "JSON 格式不正确，请检查复制内容。", "error");
      return;
    }

    const uid = findValue(parsed, ["uid", "userid", "user_id"]);
    const uuid = findValue(parsed, ["uuid", "deviceuuid", "device_uuid"]);
    if (!uid || !uuid) {
      setStatus(jsonStatus, "没有找到 uid 和 uuid，请粘贴插件通知中的完整 JSON。", "error");
      return;
    }

    uidInput.value = uid;
    uuidInput.value = uuid;
    setStatus(jsonStatus, "已填入 UID 和 UUID，请确认平台后下载。", "success");
    uidInput.focus();
  });

  document.querySelector("#clear-json").addEventListener("click", () => {
    credentialJson.value = "";
    setStatus(jsonStatus, "");
  });

  function filenameFrom(response) {
    const header = response.headers.get("Content-Disposition") || "";
    const match = header.match(/filename="([^"]+)"/i);
    return match ? match[1] : "bestdori-profile.json";
  }

  async function responseError(response) {
    try {
      const body = await response.json();
      const details = Array.isArray(body.details) ? body.details[0] : null;
      return (details && details.message) || body.message || "请求失败（" + response.status + "）";
    } catch (_) {
      return "请求失败（" + response.status + "）";
    }
  }

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    setStatus(formStatus, "");

    const uid = uidInput.value.trim();
    const uuid = uuidInput.value.trim();
    if (!/^\d{1,20}$/.test(uid)) {
      setStatus(formStatus, "请输入 1–20 位数字 UID。", "error");
      uidInput.focus();
      return;
    }
    if (!uuid) {
      setStatus(formStatus, "请输入 UUID。", "error");
      uuidInput.focus();
      return;
    }

    downloadButton.disabled = true;
    downloadButton.classList.add("is-loading");
    setStatus(formStatus, "正在读取账号资料，完成后会自动下载…");

    try {
      const headers = { "Content-Type": "application/json" };
      const apiKey = apiKeyInput.value.trim();
      if (apiKey) headers["X-API-Key"] = apiKey;

      const response = await fetch(endpoint, {
        method: "POST",
        headers,
        body: JSON.stringify({ uid, uuid, platform: platformInput.value }),
        cache: "no-store",
      });
      if (!response.ok) throw new Error(await responseError(response));

      const blob = await response.blob();
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = filenameFrom(response);
      document.body.appendChild(link);
      link.click();
      link.remove();
      URL.revokeObjectURL(url);
      setStatus(formStatus, "资料已生成，正在下载。", "success");
    } catch (error) {
      setStatus(formStatus, error.message || "导出失败，请稍后重试。", "error");
    } finally {
      downloadButton.disabled = false;
      downloadButton.classList.remove("is-loading");
    }
  });
})();
