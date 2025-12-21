---
description: Research on ideal problem space for small reasoning models (1-8B parameters)
tags:
  - research
  - llm
  - reasoning
  - benchmarks
date: 2025-12-13
---

# Small Reasoning Models Research

Research on the ideal problem space for small reasoning models (1-8B parameters), including Qwen3-4B-Thinking, DeepSeek-R1 distilled models, and Granite 4.0 hybrid/MoE architectures.

## Executive Summary

### Key Findings

1. **Scale Threshold for Native CoT**: Traditional chain-of-thought (CoT) reasoning is an emergent property requiring ~100B parameters. Small models (under 8B) produce illogical reasoning chains without specialized training.

2. **Distillation Breakthrough**: Modern distilled reasoning models (DeepSeek-R1-Distill, Qwen3-4B-Thinking) can achieve 80%+ accuracy on mathematical reasoning benchmarks by learning from larger models' reasoning patterns.

3. **Sweet Spot Complexity**: Small reasoning models excel at medium-complexity tasks (2-8 step problems) but face complete accuracy collapse beyond certain complexity thresholds.

4. **Verbosity and Loop Problems**: Reasoning models suffer from "overthinking phenomenon" - generating thousands of tokens for simple questions and potentially entering infinite reasoning loops.

5. **Hybrid Architecture Advantage**: IBM's Granite 4.0 hybrid Mamba-2/Transformer with MoE achieves 70% lower memory requirements and 2x faster inference while maintaining competitive performance.

## Task Categories

### Strong Performance

**Mathematical Reasoning** (Grade school to high school level):
- DeepSeek-R1-Distill-Qwen-1.5B: 83.9% on MATH-500
- Qwen3-4B-Thinking-2507: 97.0% on MATH-500

**Code Generation** (Small functions, API calling):
- Qwen3-4B-Thinking-2507: 71.2% on BFCL-v3

### Variable Performance

**Logical Reasoning**: Single-step to 3-step deductions work well; multi-hop reasoning degrades.

**Planning**: Generally poor under 3B; 4-8B can handle 4-6 step plans with clear structure.

### Avoid

- Long-context reasoning
- Abstract symbolic manipulation
- Open-ended creative tasks
- Multi-file codebase refactoring

## The "Sweet Spot" Problem Complexity

### Three Performance Regimes

| Regime | Complexity | Reasoning Model Performance |
|--------|------------|----------------------------|
| Low | 1-2 steps | Standard models OUTPERFORM reasoning models |
| Medium | 3-8 steps | Reasoning models show CLEAR ADVANTAGE |
| High | 10+ steps | COMPLETE ACCURACY COLLAPSE for both |

### Quantitative Boundaries

| Model Size | Optimal Steps | Max Reliable | Examples |
|-----------|---------------|--------------|----------|
| 1.5B | 2-4 | 5 | GSM8K (simple), basic function calling |
| 3-4B | 3-6 | 8 | GSM8K (full), MATH-500, BFCL-v3 |
| 7-8B | 4-8 | 12 | MATH-500, AIME (partial) |

## Chain-of-Thought Length vs Accuracy

| Task Complexity | Optimal Tokens | Impact |
|-----------------|----------------|--------|
| Simple (1-2 steps) | 0-100 | Reasoning HURTS performance |
| Medium (3-6 steps) | 100-500 | Reasoning HELPS significantly |
| Complex (7-10 steps) | 500-2000 | Helps if within capacity |
| Very Complex (10+) | 2000+ | Accuracy collapse |

## Dense vs Hybrid/MoE Models

| Model Type | Active Params | Memory | Speed | Best Use |
|-----------|---------------|--------|-------|----------|
| Dense 3B | 3B | 100% | 1x | Simple tasks, short context |
| Hybrid 3B | 3B | 30% | 2x | Long context, varied tasks |
| MoE 7B | ~1B | 15% | 3x | Complex reasoning, low resources |

### When to Choose Each

**Dense**: Simple tasks, latency-critical, context under 4k tokens

**Hybrid**: Memory-constrained, long context (32k+), multi-session inference

**MoE**: Maximum performance per active parameter, varied task complexity

## Practical Recommendations

### Model Selection

**1.5-3B Reasoning Models**: GSM8K level math, simple function calling, 2-4 step logic

**4-7B Reasoning Models**: MATH-500, multi-step word problems, API integration, code with 3-5 functions

**7B+ MoE/Hybrid**: Varied complexity, long-context, production systems

### Deployment

1. **Token Limits**: Simple=500, Medium=1000-1500, Complex=2000-3000
2. **Temperature**: Use 0.0 for math/logic, 0.1-0.3 for code
3. **Quantization**: Q8 for max accuracy, Q4 minimal reasoning impact
4. **Early Stopping**: Monitor reasoning length, stop if answer converges

## Sources

Key papers and resources:
- [Towards Reasoning Ability of Small Language Models](https://arxiv.org/html/2502.11569v3)
- [The Illusion of Thinking](https://arxiv.org/abs/2506.06941) - Problem complexity analysis
- [Stop Overthinking Survey](https://arxiv.org/html/2503.16419v4)
- [Qwen3 Technical Report](https://arxiv.org/pdf/2505.09388)
- [IBM Granite 4.0 Announcement](https://www.ibm.com/new/announcements/ibm-granite-4-0-hyper-efficient-high-performance-hybrid-models)

## See Also

- [[Help/Config/llm]] - LLM provider configuration
- [[Help/Concepts/Agents & Protocols]] - Agent integration
