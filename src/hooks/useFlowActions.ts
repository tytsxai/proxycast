import { useCallback, useState } from "react";
import {
  flowMonitorApi,
  type LLMFlow,
  type ExportFormat,
  type ExportOptions,
  getMessageText,
} from "@/lib/api/flowMonitor";

interface UseFlowActionsReturn {
  /** 复制 Flow ID */
  copyFlowId: (flowId: string) => Promise<boolean>;
  /** 复制 Flow 内容 */
  copyFlowContent: (flow: LLMFlow) => Promise<boolean>;
  /** 复制请求内容 */
  copyRequest: (flow: LLMFlow) => Promise<boolean>;
  /** 复制响应内容 */
  copyResponse: (flow: LLMFlow) => Promise<boolean>;
  /** 复制任意文本 */
  copyText: (text: string) => Promise<boolean>;
  /** 导出单个 Flow */
  exportFlow: (flowId: string, format: ExportFormat) => Promise<boolean>;
  /** 导出多个 Flow */
  exportFlows: (flowIds: string[], format: ExportFormat) => Promise<boolean>;
  /** 下载导出文件 */
  downloadExport: (data: string, filename: string, mimeType: string) => void;
  /** 是否正在导出 */
  exporting: boolean;
  /** 最后一次操作的错误 */
  error: string | null;
  /** 清除错误 */
  clearError: () => void;
}

/**
 * Flow 操作 Hook
 *
 * 提供复制和导出 Flow 的功能。
 */
export function useFlowActions(): UseFlowActionsReturn {
  const [exporting, setExporting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const clearError = useCallback(() => {
    setError(null);
  }, []);

  /**
   * 复制文本到剪贴板
   */
  const copyText = useCallback(async (text: string): Promise<boolean> => {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch (e) {
      console.error("Failed to copy:", e);
      setError("复制失败");
      return false;
    }
  }, []);

  /**
   * 复制 Flow ID
   */
  const copyFlowId = useCallback(
    async (flowId: string): Promise<boolean> => {
      return copyText(flowId);
    },
    [copyText],
  );

  /**
   * 复制 Flow 完整内容（JSON 格式）
   */
  const copyFlowContent = useCallback(
    async (flow: LLMFlow): Promise<boolean> => {
      const content = JSON.stringify(flow, null, 2);
      return copyText(content);
    },
    [copyText],
  );

  /**
   * 复制请求内容
   */
  const copyRequest = useCallback(
    async (flow: LLMFlow): Promise<boolean> => {
      // 构建可读的请求内容
      const lines: string[] = [];

      // 模型和参数
      lines.push(`模型: ${flow.request.model}`);
      lines.push(`提供商: ${flow.metadata.provider}`);
      lines.push("");

      // 系统提示词
      if (flow.request.system_prompt) {
        lines.push("=== 系统提示词 ===");
        lines.push(flow.request.system_prompt);
        lines.push("");
      }

      // 消息列表
      lines.push("=== 消息列表 ===");
      flow.request.messages.forEach((msg, index) => {
        lines.push(`[${index + 1}] ${msg.role.toUpperCase()}`);
        lines.push(getMessageText(msg.content));
        if (msg.tool_calls && msg.tool_calls.length > 0) {
          lines.push("工具调用:");
          msg.tool_calls.forEach((tc) => {
            lines.push(`  - ${tc.function.name}: ${tc.function.arguments}`);
          });
        }
        lines.push("");
      });

      // 工具定义
      if (flow.request.tools && flow.request.tools.length > 0) {
        lines.push("=== 工具定义 ===");
        flow.request.tools.forEach((tool) => {
          lines.push(`- ${tool.function.name}`);
          if (tool.function.description) {
            lines.push(`  ${tool.function.description}`);
          }
        });
        lines.push("");
      }

      return copyText(lines.join("\n"));
    },
    [copyText],
  );

  /**
   * 复制响应内容
   */
  const copyResponse = useCallback(
    async (flow: LLMFlow): Promise<boolean> => {
      if (!flow.response) {
        setError("无响应内容");
        return false;
      }

      const lines: string[] = [];

      // 响应内容
      if (flow.response.content) {
        lines.push("=== 响应内容 ===");
        lines.push(flow.response.content);
        lines.push("");
      }

      // 思维链
      if (flow.response.thinking) {
        lines.push("=== 思维链 ===");
        lines.push(flow.response.thinking.text);
        lines.push("");
      }

      // 工具调用
      if (flow.response.tool_calls.length > 0) {
        lines.push("=== 工具调用 ===");
        flow.response.tool_calls.forEach((tc) => {
          lines.push(`- ${tc.function.name}`);
          try {
            const args = JSON.parse(tc.function.arguments);
            lines.push(`  参数: ${JSON.stringify(args, null, 2)}`);
          } catch {
            lines.push(`  参数: ${tc.function.arguments}`);
          }
        });
        lines.push("");
      }

      // Token 使用
      lines.push("=== Token 使用 ===");
      lines.push(`输入: ${flow.response.usage.input_tokens}`);
      lines.push(`输出: ${flow.response.usage.output_tokens}`);
      lines.push(`总计: ${flow.response.usage.total_tokens}`);

      return copyText(lines.join("\n"));
    },
    [copyText],
  );

  /**
   * 下载导出文件
   */
  const downloadExport = useCallback(
    (data: string, filename: string, mimeType: string) => {
      const blob = new Blob([data], { type: mimeType });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = filename;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    },
    [],
  );

  /**
   * 导出单个 Flow
   */
  const exportFlow = useCallback(
    async (flowId: string, format: ExportFormat): Promise<boolean> => {
      setExporting(true);
      setError(null);

      try {
        const options: ExportOptions = {
          format,
          include_raw: true,
          include_stream_chunks: false,
          redact_sensitive: false,
        };

        const result = await flowMonitorApi.exportFlowsByIds([flowId], options);
        downloadExport(result.data, result.filename, result.mime_type);
        return true;
      } catch (e) {
        console.error("Failed to export flow:", e);
        setError(e instanceof Error ? e.message : "导出失败");
        return false;
      } finally {
        setExporting(false);
      }
    },
    [downloadExport],
  );

  /**
   * 导出多个 Flow
   */
  const exportFlows = useCallback(
    async (flowIds: string[], format: ExportFormat): Promise<boolean> => {
      if (flowIds.length === 0) {
        setError("请选择要导出的 Flow");
        return false;
      }

      setExporting(true);
      setError(null);

      try {
        const options: ExportOptions = {
          format,
          include_raw: true,
          include_stream_chunks: false,
          redact_sensitive: false,
        };

        const result = await flowMonitorApi.exportFlowsByIds(flowIds, options);
        downloadExport(result.data, result.filename, result.mime_type);
        return true;
      } catch (e) {
        console.error("Failed to export flows:", e);
        setError(e instanceof Error ? e.message : "导出失败");
        return false;
      } finally {
        setExporting(false);
      }
    },
    [downloadExport],
  );

  return {
    copyFlowId,
    copyFlowContent,
    copyRequest,
    copyResponse,
    copyText,
    exportFlow,
    exportFlows,
    downloadExport,
    exporting,
    error,
    clearError,
  };
}

export default useFlowActions;
