import { useState, useCallback, useEffect } from "react";
import {
  Activity,
  RefreshCw,
  Download,
  BarChart3,
  List,
  ChevronDown,
  Monitor,
  Maximize,
} from "lucide-react";
import {
  FlowList,
  FlowFilters,
  FlowDetail,
  FlowStats,
  ExportDialog,
} from "@/components/flow-monitor";
import {
  type LLMFlow,
  type FlowFilter,
  type ExportFormat,
} from "@/lib/api/flowMonitor";
import { windowApi, type WindowSizeOption } from "@/lib/api/window";
import { cn } from "@/lib/utils";

type ViewMode = "list" | "stats";

export function FlowMonitorPage() {
  // 视图模式
  const [viewMode, setViewMode] = useState<ViewMode>("list");

  // 过滤条件
  const [filter, setFilter] = useState<FlowFilter>({});

  // 选中的 Flow
  const [selectedFlow, setSelectedFlow] = useState<LLMFlow | null>(null);

  // 导出对话框
  const [exportDialogOpen, setExportDialogOpen] = useState(false);
  const [exportFlowIds, setExportFlowIds] = useState<string[]>([]);

  // 刷新计数器（用于触发子组件刷新）
  const [refreshKey, setRefreshKey] = useState(0);

  // 窗口大小状态
  const [windowSizeOptions, setWindowSizeOptions] = useState<
    WindowSizeOption[]
  >([]);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [showWindowMenu, setShowWindowMenu] = useState(false);

  // 初始化窗口大小选项
  useEffect(() => {
    const loadWindowOptions = async () => {
      try {
        const options = await windowApi.getWindowSizeOptions();
        setWindowSizeOptions(options);

        // 检查是否处于全屏模式
        const fullscreen = await windowApi.isFullscreen();
        setIsFullscreen(fullscreen);
      } catch (error) {
        console.error("加载窗口选项失败:", error);
      }
    };

    loadWindowOptions();
  }, []);

  // 点击外部关闭窗口菜单
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (showWindowMenu) {
        const target = event.target as Element;
        if (!target.closest(".window-menu-container")) {
          setShowWindowMenu(false);
        }
      }
    };

    document.addEventListener("mousedown", handleClickOutside);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, [showWindowMenu]);

  // 处理 Flow 选择
  const handleFlowSelect = useCallback((flow: LLMFlow) => {
    setSelectedFlow(flow);
  }, []);

  // 返回列表
  const handleBackToList = useCallback(() => {
    setSelectedFlow(null);
  }, []);

  // 刷新数据
  const handleRefresh = useCallback(() => {
    setRefreshKey((prev) => prev + 1);
  }, []);

  // 导出单个 Flow
  const handleExportFlow = useCallback(
    (flowId: string, _format: ExportFormat) => {
      setExportFlowIds([flowId]);
      setExportDialogOpen(true);
    },
    [],
  );

  // 批量导出
  const handleBatchExport = useCallback(() => {
    setExportFlowIds([]);
    setExportDialogOpen(true);
  }, []);

  // 导出成功
  const handleExportSuccess = useCallback((filename: string) => {
    console.log("导出成功:", filename);
  }, []);

  // 设置窗口大小
  const handleSetWindowSize = useCallback(async (optionId: string) => {
    try {
      await windowApi.setWindowSizeByOption(optionId);
      setShowWindowMenu(false);
    } catch (error) {
      console.error("设置窗口大小失败:", error);
    }
  }, []);

  // 切换全屏模式
  const handleToggleFullscreen = useCallback(async () => {
    try {
      const newFullscreenState = await windowApi.toggleFullscreen();
      setIsFullscreen(newFullscreenState);
      setShowWindowMenu(false);
    } catch (error) {
      console.error("切换全屏模式失败:", error);
    }
  }, []);

  return (
    <div className="space-y-6">
      {/* 页面头部 */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold flex items-center gap-2">
            <Activity className="h-6 w-6" />
            Flow Monitor
          </h2>
          <p className="text-muted-foreground">
            监控和分析 LLM API 请求/响应流量
          </p>
        </div>
        <div className="flex items-center gap-2">
          {/* 视图切换 */}
          <div className="flex items-center rounded-lg border p-1">
            <button
              onClick={() => setViewMode("list")}
              className={cn(
                "flex items-center gap-1 px-3 py-1.5 rounded text-sm transition-colors",
                viewMode === "list"
                  ? "bg-primary text-primary-foreground"
                  : "hover:bg-muted",
              )}
            >
              <List className="h-4 w-4" />
              列表
            </button>
            <button
              onClick={() => setViewMode("stats")}
              className={cn(
                "flex items-center gap-1 px-3 py-1.5 rounded text-sm transition-colors",
                viewMode === "stats"
                  ? "bg-primary text-primary-foreground"
                  : "hover:bg-muted",
              )}
            >
              <BarChart3 className="h-4 w-4" />
              统计
            </button>
          </div>

          {/* 窗口大小调整下拉菜单 */}
          <div className="relative window-menu-container">
            <button
              onClick={() => setShowWindowMenu(!showWindowMenu)}
              className="flex items-center gap-1 rounded-lg border px-3 py-2 text-sm hover:bg-muted"
              title="调整窗口大小"
            >
              <Monitor className="h-4 w-4" />
              窗口
              <ChevronDown className="h-3 w-3" />
            </button>

            {showWindowMenu && (
              <div className="absolute right-0 top-full mt-1 z-[100] min-w-[200px] rounded-lg border bg-background p-1 shadow-xl">
                {/* 窗口大小选项 */}
                <div className="px-2 py-1 text-xs font-medium text-muted-foreground">
                  窗口大小
                </div>
                {windowSizeOptions.map((option) => (
                  <button
                    key={option.id}
                    onClick={() => handleSetWindowSize(option.id)}
                    className="w-full text-left px-2 py-1.5 text-sm rounded hover:bg-accent hover:text-accent-foreground"
                  >
                    <div className="font-medium">{option.name}</div>
                    <div className="text-xs text-muted-foreground">
                      {option.description}
                    </div>
                  </button>
                ))}

                {/* 分隔线 */}
                <div className="my-1 h-px bg-border" />

                {/* 全屏选项 */}
                <button
                  onClick={handleToggleFullscreen}
                  className="w-full text-left px-2 py-1.5 text-sm rounded hover:bg-accent hover:text-accent-foreground flex items-center gap-2"
                >
                  <Maximize className="h-4 w-4" />
                  <div>
                    <div className="font-medium">
                      {isFullscreen ? "退出全屏" : "全屏模式"}
                    </div>
                    <div className="text-xs text-muted-foreground">
                      {isFullscreen ? "返回窗口模式" : "使用整个屏幕"}
                    </div>
                  </div>
                </button>
              </div>
            )}
          </div>

          {/* 导出按钮 */}
          <button
            onClick={handleBatchExport}
            className="flex items-center gap-1 rounded-lg border px-3 py-2 text-sm hover:bg-muted"
          >
            <Download className="h-4 w-4" />
            导出
          </button>

          {/* 刷新按钮 */}
          <button
            onClick={handleRefresh}
            className="flex items-center gap-1 rounded-lg border px-3 py-2 text-sm hover:bg-muted"
          >
            <RefreshCw className="h-4 w-4" />
            刷新
          </button>
        </div>
      </div>

      {/* 详情视图 */}
      {selectedFlow ? (
        <FlowDetail
          flowId={selectedFlow.id}
          onBack={handleBackToList}
          onExport={handleExportFlow}
        />
      ) : (
        <>
          {/* 过滤器 */}
          <FlowFilters filter={filter} onChange={setFilter} />

          {/* 主内容区域 */}
          {viewMode === "list" ? (
            <FlowList
              key={refreshKey}
              filter={filter}
              onFlowSelect={handleFlowSelect}
              onRefresh={handleRefresh}
              enableRealtime={true}
            />
          ) : (
            <FlowStats
              key={refreshKey}
              filter={filter}
              autoRefreshInterval={30000}
              onRefresh={handleRefresh}
            />
          )}
        </>
      )}

      {/* 导出对话框 */}
      <ExportDialog
        open={exportDialogOpen}
        onClose={() => setExportDialogOpen(false)}
        flowIds={exportFlowIds.length > 0 ? exportFlowIds : undefined}
        filter={exportFlowIds.length === 0 ? filter : undefined}
        onExportSuccess={handleExportSuccess}
      />
    </div>
  );
}

export default FlowMonitorPage;
