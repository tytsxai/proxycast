import React, { useState, useCallback } from "react";
import {
  X,
  Download,
  FileJson,
  FileText,
  FileSpreadsheet,
  FileCode,
  Loader2,
  Check,
  AlertCircle,
  Shield,
  Settings,
  ChevronDown,
  ChevronUp,
} from "lucide-react";
import {
  flowMonitorApi,
  type ExportFormat,
  type ExportOptions,
  type FlowFilter,
  type RedactionRule,
} from "@/lib/api/flowMonitor";
import { cn } from "@/lib/utils";

interface ExportDialogProps {
  /** 是否显示对话框 */
  open: boolean;
  /** 关闭对话框回调 */
  onClose: () => void;
  /** 要导出的 Flow ID 列表（批量导出） */
  flowIds?: string[];
  /** 过滤条件（按条件导出） */
  filter?: FlowFilter;
  /** 导出成功回调 */
  onExportSuccess?: (filename: string) => void;
}

interface FormatOption {
  value: ExportFormat;
  label: string;
  description: string;
  icon: React.ReactNode;
}

const FORMAT_OPTIONS: FormatOption[] = [
  {
    value: "json",
    label: "JSON",
    description: "完整的 JSON 格式，适合程序处理",
    icon: <FileJson className="h-5 w-5" />,
  },
  {
    value: "jsonl",
    label: "JSONL",
    description: "每行一个 JSON 对象，适合大数据处理",
    icon: <FileCode className="h-5 w-5" />,
  },
  {
    value: "har",
    label: "HAR",
    description: "HTTP Archive 格式，可在浏览器开发工具中查看",
    icon: <FileCode className="h-5 w-5" />,
  },
  {
    value: "markdown",
    label: "Markdown",
    description: "可读性强的文档格式，适合分享和文档",
    icon: <FileText className="h-5 w-5" />,
  },
  {
    value: "csv",
    label: "CSV",
    description: "表格格式，仅包含元数据，适合 Excel 分析",
    icon: <FileSpreadsheet className="h-5 w-5" />,
  },
];

const DEFAULT_REDACTION_RULES: RedactionRule[] = [
  {
    name: "API 密钥",
    pattern:
      "(sk-[a-zA-Z0-9]{20,}|api[_-]?key[\"']?\\s*[:=]\\s*[\"']?[a-zA-Z0-9-_]{20,})",
    replacement: "[REDACTED_API_KEY]",
    enabled: true,
  },
  {
    name: "邮箱地址",
    pattern: "[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}",
    replacement: "[REDACTED_EMAIL]",
    enabled: true,
  },
  {
    name: "手机号码",
    pattern: "1[3-9]\\d{9}",
    replacement: "[REDACTED_PHONE]",
    enabled: true,
  },
  {
    name: "Bearer Token",
    pattern: "Bearer\\s+[a-zA-Z0-9._-]+",
    replacement: "Bearer [REDACTED_TOKEN]",
    enabled: true,
  },
];

export function ExportDialog({
  open,
  onClose,
  flowIds,
  filter,
  onExportSuccess,
}: ExportDialogProps) {
  const [format, setFormat] = useState<ExportFormat>("json");
  const [includeRaw, setIncludeRaw] = useState(true);
  const [includeStreamChunks, setIncludeStreamChunks] = useState(false);
  const [redactSensitive, setRedactSensitive] = useState(false);
  const [redactionRules, setRedactionRules] = useState<RedactionRule[]>(
    DEFAULT_REDACTION_RULES,
  );
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);

  const exportCount = flowIds?.length || 0;
  const isFilterExport = !flowIds || flowIds.length === 0;

  const handleExport = useCallback(async () => {
    setExporting(true);
    setError(null);
    setSuccess(false);

    try {
      const options: ExportOptions = {
        format,
        include_raw: includeRaw,
        include_stream_chunks: includeStreamChunks,
        redact_sensitive: redactSensitive,
        redaction_rules: redactSensitive
          ? redactionRules.filter((r) => r.enabled)
          : undefined,
      };

      let result;
      if (flowIds && flowIds.length > 0) {
        // 批量导出指定 ID
        result = await flowMonitorApi.exportFlowsByIds(flowIds, options);
      } else {
        // 按过滤条件导出
        result = await flowMonitorApi.exportFlows({
          ...options,
          filter: filter || {},
        });
      }

      // 下载文件
      downloadFile(result.data, result.filename, result.mime_type);
      setSuccess(true);
      onExportSuccess?.(result.filename);

      // 延迟关闭对话框
      setTimeout(() => {
        onClose();
        setSuccess(false);
      }, 1500);
    } catch (e) {
      console.error("Export failed:", e);
      setError(e instanceof Error ? e.message : "导出失败");
    } finally {
      setExporting(false);
    }
  }, [
    format,
    includeRaw,
    includeStreamChunks,
    redactSensitive,
    redactionRules,
    flowIds,
    filter,
    onClose,
    onExportSuccess,
  ]);

  const toggleRedactionRule = (index: number) => {
    setRedactionRules((prev) =>
      prev.map((rule, i) =>
        i === index ? { ...rule, enabled: !rule.enabled } : rule,
      ),
    );
  };

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* 背景遮罩 */}
      <div className="absolute inset-0 bg-black/50" onClick={onClose} />

      {/* 对话框 */}
      <div className="relative bg-card rounded-lg shadow-xl w-full max-w-lg mx-4 max-h-[90vh] overflow-hidden flex flex-col">
        {/* 头部 */}
        <div className="flex items-center justify-between px-6 py-4 border-b">
          <div className="flex items-center gap-2">
            <Download className="h-5 w-5 text-primary" />
            <h2 className="text-lg font-semibold">导出 Flow</h2>
          </div>
          <button
            onClick={onClose}
            className="p-1 rounded hover:bg-muted"
            disabled={exporting}
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        {/* 内容 */}
        <div className="flex-1 overflow-y-auto px-6 py-4 space-y-6">
          {/* 导出数量提示 */}
          <div className="rounded-lg bg-muted/50 px-4 py-3">
            <div className="text-sm">
              {isFilterExport ? (
                <span>将导出符合当前过滤条件的所有 Flow</span>
              ) : (
                <span>
                  已选择 <strong>{exportCount}</strong> 个 Flow
                </span>
              )}
            </div>
          </div>

          {/* 格式选择 */}
          <div className="space-y-3">
            <label className="text-sm font-medium">导出格式</label>
            <div className="grid grid-cols-1 gap-2">
              {FORMAT_OPTIONS.map((option) => (
                <FormatCard
                  key={option.value}
                  option={option}
                  selected={format === option.value}
                  onClick={() => setFormat(option.value)}
                />
              ))}
            </div>
          </div>

          {/* 基本选项 */}
          <div className="space-y-3">
            <label className="text-sm font-medium">导出选项</label>
            <div className="space-y-2">
              <OptionCheckbox
                checked={includeRaw}
                onChange={setIncludeRaw}
                label="包含原始请求/响应体"
                description="导出完整的 JSON 数据"
              />
              <OptionCheckbox
                checked={includeStreamChunks}
                onChange={setIncludeStreamChunks}
                label="包含流式 Chunks"
                description="导出流式响应的原始 chunks（文件会更大）"
              />
            </div>
          </div>

          {/* 隐私选项 */}
          <div className="space-y-3">
            <div className="flex items-center gap-2">
              <Shield className="h-4 w-4 text-muted-foreground" />
              <label className="text-sm font-medium">隐私保护</label>
            </div>
            <OptionCheckbox
              checked={redactSensitive}
              onChange={setRedactSensitive}
              label="脱敏敏感数据"
              description="自动替换 API 密钥、邮箱、手机号等敏感信息"
            />

            {/* 脱敏规则 */}
            {redactSensitive && (
              <div className="ml-6 space-y-2 rounded-lg border bg-muted/30 p-3">
                <div className="text-xs text-muted-foreground mb-2">
                  脱敏规则:
                </div>
                {redactionRules.map((rule, index) => (
                  <label
                    key={rule.name}
                    className="flex items-center gap-2 text-sm cursor-pointer"
                  >
                    <input
                      type="checkbox"
                      checked={rule.enabled}
                      onChange={() => toggleRedactionRule(index)}
                      className="rounded border-gray-300"
                    />
                    <span>{rule.name}</span>
                  </label>
                ))}
              </div>
            )}
          </div>

          {/* 高级选项 */}
          <div className="space-y-3">
            <button
              onClick={() => setShowAdvanced(!showAdvanced)}
              className="flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground"
            >
              <Settings className="h-4 w-4" />
              高级选项
              {showAdvanced ? (
                <ChevronUp className="h-4 w-4" />
              ) : (
                <ChevronDown className="h-4 w-4" />
              )}
            </button>

            {showAdvanced && (
              <div className="rounded-lg border bg-muted/30 p-4 space-y-3">
                <div className="text-xs text-muted-foreground">
                  <p>• JSON/JSONL 格式适合程序处理和数据分析</p>
                  <p>• HAR 格式可在 Chrome DevTools 中导入查看</p>
                  <p>• Markdown 格式适合生成文档和分享</p>
                  <p>• CSV 格式仅包含元数据，不含消息内容</p>
                </div>
              </div>
            )}
          </div>

          {/* 错误提示 */}
          {error && (
            <div className="rounded-lg border border-red-200 bg-red-50 dark:bg-red-950/20 px-4 py-3">
              <div className="flex items-center gap-2 text-red-600 dark:text-red-400">
                <AlertCircle className="h-4 w-4" />
                <span className="text-sm">{error}</span>
              </div>
            </div>
          )}

          {/* 成功提示 */}
          {success && (
            <div className="rounded-lg border border-green-200 bg-green-50 dark:bg-green-950/20 px-4 py-3">
              <div className="flex items-center gap-2 text-green-600 dark:text-green-400">
                <Check className="h-4 w-4" />
                <span className="text-sm">导出成功！</span>
              </div>
            </div>
          )}
        </div>

        {/* 底部按钮 */}
        <div className="flex items-center justify-end gap-3 px-6 py-4 border-t bg-muted/30">
          <button
            onClick={onClose}
            disabled={exporting}
            className="px-4 py-2 text-sm rounded-lg border hover:bg-muted disabled:opacity-50"
          >
            取消
          </button>
          <button
            onClick={handleExport}
            disabled={exporting || success}
            className={cn(
              "flex items-center gap-2 px-4 py-2 text-sm rounded-lg",
              "bg-primary text-primary-foreground hover:bg-primary/90",
              "disabled:opacity-50 disabled:cursor-not-allowed",
            )}
          >
            {exporting ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" />
                导出中...
              </>
            ) : success ? (
              <>
                <Check className="h-4 w-4" />
                已完成
              </>
            ) : (
              <>
                <Download className="h-4 w-4" />
                导出
              </>
            )}
          </button>
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// 子组件
// ============================================================================

interface FormatCardProps {
  option: FormatOption;
  selected: boolean;
  onClick: () => void;
}

function FormatCard({ option, selected, onClick }: FormatCardProps) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "flex items-center gap-3 px-4 py-3 rounded-lg border text-left transition-colors",
        selected
          ? "border-primary bg-primary/5"
          : "border-transparent bg-muted/50 hover:bg-muted",
      )}
    >
      <div
        className={cn(
          "p-2 rounded-lg",
          selected
            ? "bg-primary/10 text-primary"
            : "bg-muted text-muted-foreground",
        )}
      >
        {option.icon}
      </div>
      <div className="flex-1 min-w-0">
        <div className="font-medium text-sm">{option.label}</div>
        <div className="text-xs text-muted-foreground truncate">
          {option.description}
        </div>
      </div>
      {selected && <Check className="h-5 w-5 text-primary shrink-0" />}
    </button>
  );
}

interface OptionCheckboxProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  description?: string;
}

function OptionCheckbox({
  checked,
  onChange,
  label,
  description,
}: OptionCheckboxProps) {
  return (
    <label className="flex items-start gap-3 cursor-pointer">
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        className="mt-0.5 rounded border-gray-300"
      />
      <div>
        <div className="text-sm">{label}</div>
        {description && (
          <div className="text-xs text-muted-foreground">{description}</div>
        )}
      </div>
    </label>
  );
}

// ============================================================================
// 辅助函数
// ============================================================================

/**
 * 下载文件
 */
function downloadFile(data: string, filename: string, mimeType: string) {
  const blob = new Blob([data], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

export default ExportDialog;
