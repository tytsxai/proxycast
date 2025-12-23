import { useState, useEffect } from "react";
import { Moon, Sun, Monitor, Globe, RefreshCw } from "lucide-react";
import { cn, validateProxyUrl } from "@/lib/utils";
import { getConfig, saveConfig, Config } from "@/hooks/useTauri";

type Theme = "light" | "dark" | "system";

export function GeneralSettings() {
  const [theme, setTheme] = useState<Theme>("system");
  const [launchOnStartup, setLaunchOnStartup] = useState(false);
  const [minimizeToTray, setMinimizeToTray] = useState(true);

  // 网络代理状态
  const [config, setConfig] = useState<Config | null>(null);
  const [proxyUrl, setProxyUrl] = useState<string>("");
  const [proxyError, setProxyError] = useState<string | null>(null);
  const [proxySaving, setProxySaving] = useState(false);
  const [proxyMessage, setProxyMessage] = useState<{
    type: "success" | "error";
    text: string;
  } | null>(null);
  const [configLoading, setConfigLoading] = useState(true);

  useEffect(() => {
    // 读取当前主题
    const savedTheme = localStorage.getItem("theme") as Theme | null;
    if (savedTheme) {
      setTheme(savedTheme);
    }

    // 加载配置
    loadConfig();
  }, []);

  const loadConfig = async () => {
    setConfigLoading(true);
    try {
      const c = await getConfig();
      setConfig(c);
      setProxyUrl(c.proxy_url || "");
      // 加载最小化到托盘设置
      setMinimizeToTray(c.minimize_to_tray ?? true);
    } catch (e) {
      console.error("加载配置失败:", e);
    } finally {
      setConfigLoading(false);
    }
  };

  const handleThemeChange = (newTheme: Theme) => {
    setTheme(newTheme);
    localStorage.setItem("theme", newTheme);

    // 应用主题
    const root = document.documentElement;
    if (newTheme === "system") {
      const systemDark = window.matchMedia(
        "(prefers-color-scheme: dark)",
      ).matches;
      root.classList.toggle("dark", systemDark);
    } else {
      root.classList.toggle("dark", newTheme === "dark");
    }
  };

  const handleProxyUrlChange = (value: string) => {
    setProxyUrl(value);
    if (value && !validateProxyUrl(value)) {
      setProxyError(
        "代理 URL 格式无效，请使用 http://、https:// 或 socks5:// 开头的地址",
      );
    } else {
      setProxyError(null);
    }
  };

  const handleSaveProxy = async () => {
    if (!config) return;

    // 验证格式
    if (proxyUrl && !validateProxyUrl(proxyUrl)) {
      setProxyError(
        "代理 URL 格式无效，请使用 http://、https:// 或 socks5:// 开头的地址",
      );
      return;
    }

    setProxySaving(true);
    setProxyMessage(null);

    try {
      const newConfig = {
        ...config,
        proxy_url: proxyUrl.trim() || null,
      };
      await saveConfig(newConfig);
      setConfig(newConfig);
      setProxyMessage({ type: "success", text: "代理设置已保存" });
      setTimeout(() => setProxyMessage(null), 3000);
    } catch (e: unknown) {
      const errorMessage = e instanceof Error ? e.message : String(e);
      setProxyMessage({ type: "error", text: `保存失败: ${errorMessage}` });
    } finally {
      setProxySaving(false);
    }
  };

  const themeOptions = [
    { id: "light" as Theme, label: "浅色", icon: Sun },
    { id: "dark" as Theme, label: "深色", icon: Moon },
    { id: "system" as Theme, label: "跟随系统", icon: Monitor },
  ];

  return (
    <div className="space-y-6 max-w-2xl">
      {/* 网络代理设置 */}
      <div className="space-y-4">
        <div className="flex items-center gap-2">
          <Globe className="h-5 w-5 text-blue-500" />
          <div>
            <h3 className="text-sm font-medium">网络代理</h3>
            <p className="text-xs text-muted-foreground">
              配置 HTTP/HTTPS/SOCKS5 代理，用于访问海外 API 服务
            </p>
          </div>
        </div>

        {configLoading ? (
          <div className="flex items-center justify-center p-4">
            <RefreshCw className="h-5 w-5 animate-spin text-muted-foreground" />
          </div>
        ) : (
          <div className="space-y-4 p-4 rounded-lg border">
            {/* 代理消息提示 */}
            {proxyMessage && (
              <div
                className={`rounded-lg border p-3 text-sm ${
                  proxyMessage.type === "error"
                    ? "border-destructive bg-destructive/10 text-destructive"
                    : "border-green-500 bg-green-50 text-green-700 dark:bg-green-900/20 dark:text-green-400"
                }`}
              >
                {proxyMessage.text}
              </div>
            )}

            <div>
              <label className="block text-sm font-medium mb-1.5">
                全局代理 URL
              </label>
              <input
                type="text"
                value={proxyUrl}
                onChange={(e) => handleProxyUrlChange(e.target.value)}
                placeholder="例如: http://127.0.0.1:7890 或 socks5://127.0.0.1:1080"
                className={cn(
                  "w-full px-3 py-2 rounded-lg border bg-background text-sm focus:ring-2 focus:ring-primary/20 focus:border-primary outline-none",
                  proxyError &&
                    "border-destructive focus:border-destructive focus:ring-destructive/20",
                )}
              />
              {proxyError ? (
                <p className="text-xs text-destructive mt-1">{proxyError}</p>
              ) : (
                <p className="text-xs text-muted-foreground mt-1">
                  留空表示不使用代理。支持 http://、https://、socks5:// 协议
                </p>
              )}
            </div>

            <div className="rounded-lg bg-blue-50 dark:bg-blue-900/20 p-3 text-sm">
              <p className="font-medium text-blue-700 dark:text-blue-300">
                代理优先级说明：
              </p>
              <ul className="mt-1 list-inside list-disc text-blue-600 dark:text-blue-400 text-xs">
                <li>凭证级代理优先于全局代理</li>
                <li>如果凭证未设置代理，则使用此全局代理</li>
                <li>全局代理为空时，直接连接 API 服务</li>
              </ul>
            </div>

            <button
              onClick={handleSaveProxy}
              disabled={proxySaving || !!proxyError}
              className="w-full px-4 py-2 rounded-lg bg-primary text-primary-foreground text-sm font-medium hover:bg-primary/90 disabled:opacity-50"
            >
              {proxySaving ? "保存中..." : "保存代理设置"}
            </button>
          </div>
        )}
      </div>

      {/* 主题设置 */}
      <div className="space-y-3">
        <div>
          <h3 className="text-sm font-medium">主题</h3>
          <p className="text-xs text-muted-foreground">选择界面显示主题</p>
        </div>
        <div className="flex gap-2">
          {themeOptions.map((option) => (
            <button
              key={option.id}
              onClick={() => handleThemeChange(option.id)}
              className={cn(
                "flex items-center gap-2 px-4 py-2 rounded-lg border transition-colors",
                theme === option.id
                  ? "border-primary bg-primary/10 text-primary"
                  : "border-border hover:border-muted-foreground/50",
              )}
            >
              <option.icon className="h-4 w-4" />
              <span className="text-sm">{option.label}</span>
            </button>
          ))}
        </div>
      </div>

      {/* 启动设置 */}
      <div className="space-y-4">
        <div>
          <h3 className="text-sm font-medium">启动行为</h3>
          <p className="text-xs text-muted-foreground">
            配置应用启动和关闭行为
          </p>
        </div>

        <div className="space-y-3">
          <label className="flex items-center justify-between p-3 rounded-lg border cursor-pointer hover:bg-muted/50">
            <div>
              <span className="text-sm font-medium">开机自启动</span>
              <p className="text-xs text-muted-foreground">
                系统启动时自动运行 ProxyCast
              </p>
            </div>
            <input
              type="checkbox"
              checked={launchOnStartup}
              onChange={(e) => setLaunchOnStartup(e.target.checked)}
              className="w-4 h-4 rounded border-gray-300"
            />
          </label>

          <label className="flex items-center justify-between p-3 rounded-lg border cursor-pointer hover:bg-muted/50">
            <div>
              <span className="text-sm font-medium">关闭时最小化到托盘</span>
              <p className="text-xs text-muted-foreground">
                点击关闭按钮时最小化而不是退出
              </p>
            </div>
            <input
              type="checkbox"
              checked={minimizeToTray}
              onChange={async (e) => {
                const newValue = e.target.checked;
                setMinimizeToTray(newValue);
                if (config) {
                  try {
                    await saveConfig({ ...config, minimize_to_tray: newValue });
                    setConfig({ ...config, minimize_to_tray: newValue });
                  } catch (err) {
                    console.error("保存最小化到托盘设置失败:", err);
                    // 恢复原值
                    setMinimizeToTray(!newValue);
                  }
                }
              }}
              className="w-4 h-4 rounded border-gray-300"
            />
          </label>
        </div>
      </div>
    </div>
  );
}
