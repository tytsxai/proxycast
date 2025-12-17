---
title: ProxyCast
description: 把你的 AI 客户端额度用到任何地方
navigation: false
layout: page
---

::hero
---
announcement:
  title: 🎉 ProxyCast v1.0 发布
  icon: i-heroicons-megaphone
  to: /introduction/overview
actions:
  - label: 快速开始
    icon: i-heroicons-rocket-launch
    to: /introduction/quickstart
    color: primary
  - label: GitHub
    icon: i-simple-icons-github
    to: https://github.com/aiclientproxy/proxycast
    target: _blank
    color: neutral
---

#title
ProxyCast

#description
把你的 AI 客户端额度用到任何地方。一款基于 Tauri 的桌面应用，将 Kiro、Gemini CLI、Qwen 等 AI 客户端凭证转换为标准 OpenAI/Claude 兼容 API。
::

::alert{type="warning"}
**免责声明**: 本工具仅限于个人合法使用，严禁用于非法盈利目的。初衷是帮助用户充分利用已订阅的 AI 服务 Token。[查看完整声明](/legal/disclaimer)
::

::card-group
  ::card
  ---
  title: 凭证池管理
  icon: i-heroicons-key
  to: /user-guide/credential-pool
  ---
  支持多种 AI 客户端凭证的统一管理，包括 Kiro、Gemini CLI、Qwen、Claude Code 等。
  ::

  ::card
  ---
  title: 智能路由
  icon: i-heroicons-arrows-right-left
  to: /user-guide/smart-routing
  ---
  基于负载均衡、优先级、健康检查的智能请求路由策略。
  ::

  ::card
  ---
  title: 容错配置
  icon: i-heroicons-shield-check
  to: /user-guide/resilience
  ---
  内置熔断器、重试机制、超时控制，确保服务稳定性。
  ::

  ::card
  ---
  title: 配置切换
  icon: i-heroicons-cog-6-tooth
  to: /user-guide/config-switch
  ---
  一键切换 Claude Code、Codex、Gemini CLI 等客户端配置。
  ::
::

::section
#title
核心特性

#description
ProxyCast 提供完整的 AI 客户端代理解决方案

::card-group
  ::card
  ---
  title: 仪表盘
  icon: i-heroicons-chart-bar
  to: /user-guide/dashboard
  ---
  实时监控请求统计、凭证状态、系统健康度。
  ::

  ::card
  ---
  title: 监控中心
  icon: i-heroicons-eye
  to: /user-guide/monitoring
  ---
  详细的请求日志、性能指标、错误追踪。
  ::

  ::card
  ---
  title: API Server
  icon: i-heroicons-server
  to: /user-guide/api-server
  ---
  OpenAI/Claude 兼容的 API 服务端点。
  ::

  ::card
  ---
  title: MCP 支持
  icon: i-heroicons-puzzle-piece
  to: /user-guide/mcp
  ---
  Model Context Protocol 集成支持。
  ::

  ::card
  ---
  title: Prompts 管理
  icon: i-heroicons-document-text
  to: /user-guide/prompts
  ---
  系统提示词模板管理与复用。
  ::

  ::card
  ---
  title: Skills 技能
  icon: i-heroicons-sparkles
  to: /user-guide/skills
  ---
  可扩展的技能模块系统。
  ::
::
::
