import React from "react";
import {
  Clock,
  ArrowRight,
  CheckCircle2,
  XCircle,
  Loader2,
  Zap,
  Send,
  Download,
} from "lucide-react";
import type { LLMFlow, FlowTimestamps } from "@/lib/api/flowMonitor";
import { formatLatency } from "@/lib/api/flowMonitor";
import { cn } from "@/lib/utils";

interface FlowTimelineProps {
  flow: LLMFlow;
  className?: string;
}

interface TimelineEvent {
  id: string;
  label: string;
  timestamp: Date | null;
  icon: React.ReactNode;
  color: string;
  duration?: number;
  durationLabel?: string;
}

export function FlowTimeline({ flow, className }: FlowTimelineProps) {
  const { timestamps, state, response } = flow;

  // 构建时间线事件
  const events = buildTimelineEvents(timestamps, state, response?.stream_info);

  // 计算时间范围
  const validTimestamps = events
    .filter((e) => e.timestamp !== null)
    .map((e) => e.timestamp!.getTime());

  if (validTimestamps.length === 0) {
    return (
      <div className={cn("rounded-lg border bg-card p-4", className)}>
        <div className="text-center text-muted-foreground">暂无时间线数据</div>
      </div>
    );
  }

  const minTime = Math.min(...validTimestamps);
  const maxTime = Math.max(...validTimestamps);
  const totalDuration = maxTime - minTime;

  return (
    <div className={cn("rounded-lg border bg-card p-4", className)}>
      <h3 className="text-sm font-medium mb-4 flex items-center gap-2">
        <Clock className="h-4 w-4" />
        请求时间线
      </h3>

      {/* 总耗时 */}
      <div className="mb-4 flex items-center justify-between text-sm">
        <span className="text-muted-foreground">总耗时</span>
        <span className="font-medium">
          {formatLatency(timestamps.duration_ms)}
        </span>
      </div>

      {/* 时间线可视化 */}
      <div className="relative">
        {/* 时间轴背景 */}
        <div className="absolute left-6 top-0 bottom-0 w-0.5 bg-muted" />

        {/* 事件列表 */}
        <div className="space-y-4">
          {events.map((event) => (
            <TimelineEventItem
              key={event.id}
              event={event}
              totalDuration={totalDuration}
              minTime={minTime}
            />
          ))}
        </div>
      </div>

      {/* 时间分布条 */}
      <TimelineBar
        timestamps={timestamps}
        streamInfo={response?.stream_info}
        className="mt-6"
      />
    </div>
  );
}

function buildTimelineEvents(
  timestamps: FlowTimestamps,
  state: string,
  streamInfo?: { first_chunk_latency_ms: number; chunk_count: number },
): TimelineEvent[] {
  const events: TimelineEvent[] = [];

  // 创建时间
  events.push({
    id: "created",
    label: "Flow 创建",
    timestamp: new Date(timestamps.created),
    icon: <Clock className="h-4 w-4" />,
    color: "text-gray-500",
  });

  // 请求开始
  events.push({
    id: "request_start",
    label: "请求开始",
    timestamp: new Date(timestamps.request_start),
    icon: <Send className="h-4 w-4" />,
    color: "text-blue-500",
  });

  // 请求结束
  if (timestamps.request_end) {
    const requestDuration =
      new Date(timestamps.request_end).getTime() -
      new Date(timestamps.request_start).getTime();
    events.push({
      id: "request_end",
      label: "请求发送完成",
      timestamp: new Date(timestamps.request_end),
      icon: <ArrowRight className="h-4 w-4" />,
      color: "text-blue-500",
      duration: requestDuration,
      durationLabel: `请求耗时 ${formatLatency(requestDuration)}`,
    });
  }

  // 响应开始 (TTFB)
  if (timestamps.response_start) {
    events.push({
      id: "response_start",
      label: "首字节到达 (TTFB)",
      timestamp: new Date(timestamps.response_start),
      icon: <Download className="h-4 w-4" />,
      color: "text-green-500",
      duration: timestamps.ttfb_ms,
      durationLabel: timestamps.ttfb_ms
        ? `TTFB ${formatLatency(timestamps.ttfb_ms)}`
        : undefined,
    });
  }

  // 流式响应信息
  if (streamInfo && streamInfo.chunk_count > 0) {
    events.push({
      id: "streaming",
      label: `流式传输 (${streamInfo.chunk_count} chunks)`,
      timestamp: timestamps.response_start
        ? new Date(timestamps.response_start)
        : null,
      icon: <Loader2 className="h-4 w-4" />,
      color: "text-purple-500",
      durationLabel: `首 chunk ${formatLatency(streamInfo.first_chunk_latency_ms)}`,
    });
  }

  // 响应结束
  if (timestamps.response_end) {
    const isSuccess = state === "Completed";
    const isFailed = state === "Failed";

    events.push({
      id: "response_end",
      label: isSuccess ? "响应完成" : isFailed ? "请求失败" : "响应结束",
      timestamp: new Date(timestamps.response_end),
      icon: isSuccess ? (
        <CheckCircle2 className="h-4 w-4" />
      ) : isFailed ? (
        <XCircle className="h-4 w-4" />
      ) : (
        <Zap className="h-4 w-4" />
      ),
      color: isSuccess
        ? "text-green-500"
        : isFailed
          ? "text-red-500"
          : "text-gray-500",
      duration: timestamps.duration_ms,
      durationLabel: `总耗时 ${formatLatency(timestamps.duration_ms)}`,
    });
  }

  return events;
}

interface TimelineEventItemProps {
  event: TimelineEvent;
  totalDuration: number;
  minTime: number;
}

function TimelineEventItem({
  event,
  totalDuration,
  minTime,
}: TimelineEventItemProps) {
  const formatTime = (date: Date | null) => {
    if (!date) return "-";
    // 格式化时间，包含毫秒
    const hours = date.getHours().toString().padStart(2, "0");
    const minutes = date.getMinutes().toString().padStart(2, "0");
    const seconds = date.getSeconds().toString().padStart(2, "0");
    const ms = date.getMilliseconds().toString().padStart(3, "0");
    return `${hours}:${minutes}:${seconds}.${ms}`;
  };

  // 计算相对位置百分比（保留用于未来可能的动画效果）
  void (event.timestamp && totalDuration > 0
    ? ((event.timestamp.getTime() - minTime) / totalDuration) * 100
    : 0);

  return (
    <div className="relative flex items-start gap-3">
      {/* 时间点标记 */}
      <div
        className={cn(
          "relative z-10 flex h-8 w-8 items-center justify-center rounded-full bg-card border-2",
          event.color.replace("text-", "border-"),
        )}
      >
        <span className={event.color}>{event.icon}</span>
      </div>

      {/* 事件内容 */}
      <div className="flex-1 min-w-0 pt-1">
        <div className="flex items-center justify-between gap-2">
          <span className="text-sm font-medium">{event.label}</span>
          <span className="text-xs text-muted-foreground font-mono">
            {formatTime(event.timestamp)}
          </span>
        </div>
        {event.durationLabel && (
          <div className="text-xs text-muted-foreground mt-0.5">
            {event.durationLabel}
          </div>
        )}
      </div>
    </div>
  );
}

// ============================================================================
// 时间分布条组件
// ============================================================================

interface TimelineBarProps {
  timestamps: FlowTimestamps;
  streamInfo?: { first_chunk_latency_ms: number; chunk_count: number };
  className?: string;
}

function TimelineBar({ timestamps, streamInfo, className }: TimelineBarProps) {
  const totalDuration = timestamps.duration_ms;

  if (totalDuration === 0) {
    return null;
  }

  // 计算各阶段占比
  const phases: {
    id: string;
    label: string;
    duration: number;
    color: string;
    percentage: number;
  }[] = [];

  // 请求发送阶段
  if (timestamps.request_end) {
    const requestDuration =
      new Date(timestamps.request_end).getTime() -
      new Date(timestamps.request_start).getTime();
    if (requestDuration > 0) {
      phases.push({
        id: "request",
        label: "请求发送",
        duration: requestDuration,
        color: "bg-blue-500",
        percentage: (requestDuration / totalDuration) * 100,
      });
    }
  }

  // 等待响应阶段 (TTFB)
  if (timestamps.ttfb_ms && timestamps.request_end) {
    const waitDuration =
      timestamps.ttfb_ms -
      (new Date(timestamps.request_end).getTime() -
        new Date(timestamps.request_start).getTime());
    if (waitDuration > 0) {
      phases.push({
        id: "wait",
        label: "等待响应",
        duration: waitDuration,
        color: "bg-yellow-500",
        percentage: (waitDuration / totalDuration) * 100,
      });
    }
  } else if (timestamps.ttfb_ms) {
    phases.push({
      id: "ttfb",
      label: "TTFB",
      duration: timestamps.ttfb_ms,
      color: "bg-yellow-500",
      percentage: (timestamps.ttfb_ms / totalDuration) * 100,
    });
  }

  // 响应接收阶段
  if (timestamps.response_start && timestamps.response_end) {
    const responseDuration =
      new Date(timestamps.response_end).getTime() -
      new Date(timestamps.response_start).getTime();
    if (responseDuration > 0) {
      phases.push({
        id: "response",
        label: streamInfo ? "流式接收" : "响应接收",
        duration: responseDuration,
        color: streamInfo ? "bg-purple-500" : "bg-green-500",
        percentage: (responseDuration / totalDuration) * 100,
      });
    }
  }

  // 如果没有详细阶段，显示总时间
  if (phases.length === 0) {
    phases.push({
      id: "total",
      label: "总耗时",
      duration: totalDuration,
      color: "bg-gray-500",
      percentage: 100,
    });
  }

  return (
    <div className={cn("space-y-2", className)}>
      <div className="text-xs text-muted-foreground">时间分布</div>

      {/* 进度条 */}
      <div className="h-3 rounded-full bg-muted overflow-hidden flex">
        {phases.map((phase) => (
          <div
            key={phase.id}
            className={cn("h-full transition-all", phase.color)}
            style={{ width: `${Math.max(phase.percentage, 1)}%` }}
            title={`${phase.label}: ${formatLatency(phase.duration)} (${phase.percentage.toFixed(1)}%)`}
          />
        ))}
      </div>

      {/* 图例 */}
      <div className="flex flex-wrap gap-3 text-xs">
        {phases.map((phase) => (
          <div key={phase.id} className="flex items-center gap-1.5">
            <div className={cn("w-2.5 h-2.5 rounded-sm", phase.color)} />
            <span className="text-muted-foreground">{phase.label}</span>
            <span className="font-medium">{formatLatency(phase.duration)}</span>
            <span className="text-muted-foreground">
              ({phase.percentage.toFixed(1)}%)
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ============================================================================
// 简化版时间线（用于列表预览）
// ============================================================================

interface FlowTimelineCompactProps {
  timestamps: FlowTimestamps;
  state: string;
  className?: string;
}

export function FlowTimelineCompact({
  timestamps,
  state,
  className,
}: FlowTimelineCompactProps) {
  const totalDuration = timestamps.duration_ms;
  const ttfb = timestamps.ttfb_ms || 0;

  // 计算 TTFB 占比
  const ttfbPercentage = totalDuration > 0 ? (ttfb / totalDuration) * 100 : 0;
  const responsePercentage = 100 - ttfbPercentage;

  const isSuccess = state === "Completed";
  const isFailed = state === "Failed";

  return (
    <div className={cn("space-y-1", className)}>
      <div className="h-1.5 rounded-full bg-muted overflow-hidden flex">
        {ttfb > 0 && (
          <div
            className="h-full bg-yellow-500"
            style={{ width: `${ttfbPercentage}%` }}
            title={`TTFB: ${formatLatency(ttfb)}`}
          />
        )}
        <div
          className={cn(
            "h-full",
            isSuccess
              ? "bg-green-500"
              : isFailed
                ? "bg-red-500"
                : "bg-blue-500",
          )}
          style={{ width: `${responsePercentage}%` }}
          title={`响应: ${formatLatency(totalDuration - ttfb)}`}
        />
      </div>
      <div className="flex justify-between text-xs text-muted-foreground">
        <span>{ttfb > 0 ? `TTFB ${formatLatency(ttfb)}` : ""}</span>
        <span>{formatLatency(totalDuration)}</span>
      </div>
    </div>
  );
}

export default FlowTimeline;
