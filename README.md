# 盘古 (Pangu)

> 开天辟地，一创世纪。盘古是 Rust 写的元 Agent，能用工具、有记忆、会自我进化，真正能干活的 AI Agent。

## 设计理念

大多数 Agent 停留在"能对话"的层面。盘古的目标是：**能干活、能学习、能进化**。

- **能干活**：完整的工具调用系统，支持 Shell/Web/文件/API
- **有记忆**：分层记忆架构，不丢上下文，能跨 session 积累知识
- **会进化**：从错误中学习，自动生成新技能，能力持续增长

## 核心架构

```
┌─────────────────────────────────────────────────────────┐
│                      Agent Core                         │
│  ┌─────────┐    ┌──────────┐    ┌───────────────────┐  │
│  │ Planner │───▶│ Executor │───▶│    Observer       │  │
│  │ (推理)  │    │ (执行)   │    │ (自省/评估)       │  │
│  └─────────┘    └──────────┘    └─────────┬─────────┘  │
│       ▲                                  │             │
│       │ 反馈循环                          ▼             │
│  ┌──────────────────────────────────────────────┐     │
│  │              Evolution Engine                 │     │
│  │  ┌──────────────┐  ┌────────────────────┐    │     │
│  │  │ ErrorCollector│  │ SkillGenerator    │    │     │
│  │  │ (错误收集)    │  │ (技能生成)        │    │     │
│  │  └──────────────┘  └────────────────────┘    │     │
│  └──────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────┘
         │                │                │
         ▼                ▼                ▼
┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│ Tool System  │  │Memory System │  │  LLM Bridge  │
│              │  │              │  │              │
│ ┌──────────┐ │  │ ┌──────────┐ │  │ OpenAI       │
│ │Registry  │ │  │ │Working   │ │  │ Anthropic    │
│ │Executor  │ │  │ │Memory    │ │  │ Local(Ollama)│
│ │Reflector │ │  │ ├──────────┤ │  │              │
│ └──────────┘ │  │ │Episodic  │ │  └──────────────┘
└──────────────┘  │ │(SQLite)  │ │
                  │ ├──────────┤ │
                  │ │Semantic  │ │
                  │ │(VectorDB)│ │
                  │ └──────────┘ │
                  └──────────────┘
```

## 四大核心模块

### 1. Agent Core（主循环）

事件驱动的 Agent 主循环，包含三个子组件：

- **Planner**：接收任务，使用 ReAct（Reasoning + Acting）模式分解子任务，做决策
- **Executor**：根据计划调用工具，收集执行结果
- **Observer**：评估执行结果是否符合预期，记录成功/失败模式

### 2. Tool System（工具系统）

动态工具注册和执行框架：

- **Tool Registry**：维护工具注册表，每个工具有名称/描述/参数 schema
- **Tool Executor**：隔离执行工具（Sandbox），超时控制，错误捕获
- **Tool Reflector**：工具执行后评估是否达到目的

内置工具：
- `shell` — 执行 Shell 命令
- `read_file` / `write_file` / `edit_file` — 文件操作
- `web_search` / `web_fetch` — Web 搜索和抓取
- `http_request` — 通用 HTTP 请求
- `memory_search` / `memory_write` — 记忆读写

### 3. Memory System（分层记忆）

三层记忆，模拟人类认知架构：

| 层 | 存储 | 生命周期 | 容量 |
|----|------|----------|------|
| **Working Memory** | 内存 | Session | ~128K tokens |
| **Episodic Memory** | SQLite | 短中期 | 无限 |
| **Semantic Memory** | Vector DB | 长期 | 无限 |

- **Working Memory**：当前会话上下文，随对话动态增长，超窗口压缩
- **Episodic Memory**：完整事件记录（任务+结果+评估），可精确回溯
- **Semantic Memory**：提取的事实/模式，用 Embedding 存储，支持语义搜索

### 4. Evolution Engine（进化引擎）

盘古的核心创新——从实践中学习：

```
执行失败 → ErrorCollector 记录（错误类型+上下文+尝试次数）
              ↓
         SelfCorrector 分析模式，生成修正策略
              ↓
         SkillGenerator 将成功策略写成新 Skill
              ↓
         ToolRegistry 注册新工具，下次直接调用
```

- **ErrorCollector**：每次失败记录到 `~/.pangu/errors.jsonl`
- **SelfCorrector**：分析错误模式，决定是否需要生成新技能
- **SkillGenerator**：用 LLM 生成可执行的 Rust 工具代码

## ReAct Agent 循环

```
loop {
    1. Think:     LLM 推理当前状态，决定下一步行动
    2. Act:       选择工具并执行（带参数）
    3. Observe:   收集执行结果，评估是否达成目标
    4. Adapt:     若失败，记录错误，若成功，继续或结束
}
```

## 配置

```yaml
# ~/.pangu/config.yaml
llm:
  provider: "openai"        # openai | anthropic | ollama
  model: "gpt-4o"
  api_key: "${OPENAI_API_KEY}"

memory:
  working_window: 128000    # tokens
  episodic_db: "~/.pangu/episodic.db"
  vector_db: "~/.pangu/vectors"

tools:
  allowed:
    - shell
    - read_file
    - write_file
    - web_search
  shell_timeout: 30         # seconds

evolution:
  enabled: true
  error_threshold: 3         # 失败3次后触发技能生成
  skill_output: "~/.pangu/skills/"
```

## 快速开始

```bash
# 安装
git clone https://github.com/zhugezihou/pangu.git
cd pangu
cargo build --release

# 配置
cp config.example.yaml ~/.pangu/config.yaml
vim ~/.pangu/config.yaml  # 填入 API key

# 运行
cargo run --release
# 或者交互式
cargo run --release -- --interactive

# 单次任务
cargo run --release -- --task "帮我查一下今天北京的天气，写到 ~/weather.txt"
```

## 项目结构

```
pangu/
├── src/
│   ├── main.rs              # 入口
│   ├── lib.rs               # 库入口
│   ├── core/
│   │   ├── mod.rs
│   │   ├── agent.rs         # Agent 主循环（ReAct）
│   │   ├── planner.rs       # 规划器
│   │   ├── executor.rs      # 执行器
│   │   └── observer.rs      # 自省器
│   ├── tools/
│   │   ├── mod.rs
│   │   ├── registry.rs      # 工具注册表
│   │   ├── executor.rs      # 工具执行器
│   │   └── builtins/        # 内置工具
│   │       ├── mod.rs
│   │       ├── shell.rs
│   │       ├── file.rs
│   │       └── web.rs
│   ├── memory/
│   │   ├── mod.rs
│   │   ├── working.rs       # 工作记忆
│   │   ├── episodic.rs      # 情景记忆（SQLite）
│   │   └── semantic.rs      # 语义记忆（向量）
│   ├── evolution/
│   │   ├── mod.rs
│   │   ├── error_collector.rs
│   │   ├── self_corrector.rs
│   │   └── skill_generator.rs
│   ├── llm/
│   │   ├── mod.rs
│   │   ├── provider.rs      # LLM provider 抽象
│   │   ├── openai.rs
│   │   ├── anthropic.rs
│   │   └── ollama.rs
│   └── session/
│       ├── mod.rs
│       ├── context.rs       # 上下文管理
│       └── compression.rs   # 上下文压缩
├── tests/
├── skills/                  # 生成的技能（可导入）
├── Cargo.toml
├── config.example.yaml
└── README.md
```

## 进化演示

盘古遇到未知任务时的进化路径：

```
第1次：任务X → 失败（无工具）→ ErrorCollector 记录
第2次：任务X → 仍失败 → SelfCorrector 分析：需要新工具
第3次：任务X → SkillGenerator 生成 tool_x.rs
                  ↓
         编译 + 注册到 Registry
                  ↓
第4次：任务X → tool_x 成功执行 → 记录到 Episodic Memory
```

## 对比

| 能力 | 盘古 | LangChain | AutoGPT | ChatGPT |
|------|------|-----------|---------|---------|
| 工具调用 | ✅ Rust native | ✅ Python | ✅ Python | ❌ |
| 记忆分层 | ✅ 三层 | ⚠️ 向量DB | ⚠️ 简单 | ❌ |
| 自我进化 | ✅ 自动生成技能 | ❌ | ❌ | ❌ |
| 编译安全 | ✅ 零运行时 | ❌ Python | ❌ Python | ❌ |
| 上下文压缩 | ✅ LLM 摘要 | ⚠️ | ⚠️ | ❌ |

## License

MIT
