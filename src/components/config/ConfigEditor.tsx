import React, { useState, useEffect, useCallback, useRef } from "react";
import { AlertCircle, Check, Copy, FileCode } from "lucide-react";
import { Config, configApi } from "@/lib/api/config";

interface ConfigEditorProps {
  config: Config | null;
  onConfigChange: (config: Config) => void;
}

// Simple YAML syntax highlighter
function escapeHtmlForYamlHighlight(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function highlightYaml(yaml: string): string {
  const escaped = escapeHtmlForYamlHighlight(yaml);
  return (
    escaped
      // Comments
      .replace(/(#.*$)/gm, '<span class="yaml-comment">$1</span>')
      // Keys (before colon)
      .replace(
        /^(\s*)([a-zA-Z_][a-zA-Z0-9_]*):/gm,
        '$1<span class="yaml-key">$2</span>:',
      )
      // Strings in quotes
      .replace(
        /"([^"\\]*(\\.[^"\\]*)*)"/g,
        '<span class="yaml-string">"$1"</span>',
      )
      .replace(
        /'([^'\\]*(\\.[^'\\]*)*)'/g,
        "<span class=\"yaml-string\">'$1'</span>",
      )
      // Booleans
      .replace(
        /:\s*(true|false)(\s|$)/gi,
        ': <span class="yaml-boolean">$1</span>$2',
      )
      // Numbers
      .replace(
        /:\s*(\d+\.?\d*)(\s|$)/g,
        ': <span class="yaml-number">$1</span>$2',
      )
      // Null
      .replace(/:\s*(null|~)(\s|$)/gi, ': <span class="yaml-null">$1</span>$2')
  );
}

export function ConfigEditor({ config, onConfigChange }: ConfigEditorProps) {
  const [yamlContent, setYamlContent] = useState("");
  const [highlightedContent, setHighlightedContent] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isValid, setIsValid] = useState(true);
  const [copied, setCopied] = useState(false);
  const [isValidating, setIsValidating] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const highlightRef = useRef<HTMLPreElement>(null);
  const validateTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Load YAML from config
  const loadYamlFromConfig = useCallback(async () => {
    if (!config) return;
    try {
      const result = await configApi.exportConfig(config, false);
      setYamlContent(result.content);
      setHighlightedContent(highlightYaml(result.content));
      setError(null);
      setIsValid(true);
    } catch (err) {
      setError(`加载配置失败: ${err}`);
    }
  }, [config]);

  // Load initial YAML from config
  useEffect(() => {
    if (config) {
      loadYamlFromConfig();
    }
  }, [config, loadYamlFromConfig]);

  // Validate YAML with debounce
  const validateYaml = useCallback(
    async (content: string) => {
      if (!content.trim()) {
        setError(null);
        setIsValid(false);
        return;
      }

      setIsValidating(true);
      try {
        const validatedConfig = await configApi.validateConfigYaml(content);
        setError(null);
        setIsValid(true);
        onConfigChange(validatedConfig);
      } catch (err) {
        setError(`${err}`);
        setIsValid(false);
      } finally {
        setIsValidating(false);
      }
    },
    [onConfigChange],
  );

  // Handle content change with debounced validation
  const handleContentChange = (newContent: string) => {
    setYamlContent(newContent);
    setHighlightedContent(highlightYaml(newContent));

    // Clear previous timeout
    if (validateTimeoutRef.current) {
      clearTimeout(validateTimeoutRef.current);
    }

    // Debounce validation
    validateTimeoutRef.current = setTimeout(() => {
      validateYaml(newContent);
    }, 500);
  };

  // Sync scroll between textarea and highlight
  const handleScroll = () => {
    if (textareaRef.current && highlightRef.current) {
      highlightRef.current.scrollTop = textareaRef.current.scrollTop;
      highlightRef.current.scrollLeft = textareaRef.current.scrollLeft;
    }
  };

  // Copy to clipboard
  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(yamlContent);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      setError("复制失败");
    }
  };

  // Handle tab key for indentation
  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Tab") {
      e.preventDefault();
      const textarea = e.currentTarget;
      const start = textarea.selectionStart;
      const end = textarea.selectionEnd;
      const newContent =
        yamlContent.substring(0, start) + "  " + yamlContent.substring(end);
      handleContentChange(newContent);
      // Restore cursor position
      setTimeout(() => {
        textarea.selectionStart = textarea.selectionEnd = start + 2;
      }, 0);
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <FileCode className="h-5 w-5" />
          <h3 className="text-lg font-medium">YAML 配置编辑器</h3>
        </div>
        <div className="flex items-center gap-2">
          {isValidating && (
            <span className="text-sm text-muted-foreground">验证中...</span>
          )}
          {!isValidating && isValid && yamlContent && (
            <span className="flex items-center gap-1 text-sm text-green-600">
              <Check className="h-4 w-4" />
              有效
            </span>
          )}
          <button
            onClick={handleCopy}
            className="flex items-center gap-1 rounded-lg border px-3 py-1.5 text-sm hover:bg-muted"
          >
            {copied ? (
              <>
                <Check className="h-4 w-4" />
                已复制
              </>
            ) : (
              <>
                <Copy className="h-4 w-4" />
                复制
              </>
            )}
          </button>
        </div>
      </div>

      {/* Editor container */}
      <div className="relative rounded-lg border bg-[#1e1e1e] overflow-hidden">
        {/* Syntax highlighted layer */}
        <pre
          ref={highlightRef}
          className="yaml-highlight absolute inset-0 p-4 m-0 overflow-auto pointer-events-none font-mono text-sm leading-6 whitespace-pre-wrap break-words"
          aria-hidden="true"
          dangerouslySetInnerHTML={{ __html: highlightedContent + "\n" }}
        />
        {/* Editable textarea */}
        <textarea
          ref={textareaRef}
          value={yamlContent}
          onChange={(e) => handleContentChange(e.target.value)}
          onScroll={handleScroll}
          onKeyDown={handleKeyDown}
          className="relative w-full h-[500px] p-4 font-mono text-sm leading-6 bg-transparent text-transparent caret-white resize-none outline-none"
          spellCheck={false}
          placeholder="# 在此输入 YAML 配置..."
        />
      </div>

      {/* Error display */}
      {error && (
        <div className="flex items-start gap-2 rounded-lg border border-red-200 bg-red-50 p-3 text-red-700 dark:border-red-800 dark:bg-red-950 dark:text-red-400">
          <AlertCircle className="h-5 w-5 flex-shrink-0 mt-0.5" />
          <div className="text-sm">
            <p className="font-medium">配置错误</p>
            <p className="mt-1 whitespace-pre-wrap">{error}</p>
          </div>
        </div>
      )}

      {/* Syntax highlighting styles */}
      <style>{`
        .yaml-highlight {
          color: #d4d4d4;
        }
        .yaml-key {
          color: #9cdcfe;
        }
        .yaml-string {
          color: #ce9178;
        }
        .yaml-number {
          color: #b5cea8;
        }
        .yaml-boolean {
          color: #569cd6;
        }
        .yaml-null {
          color: #569cd6;
        }
        .yaml-comment {
          color: #6a9955;
        }
      `}</style>
    </div>
  );
}
