# Research: Ideal Problem Space for Small Reasoning Models (1-8B Parameters)

**Research Date**: 2025-12-13
**Models of Interest**: Qwen3-4B-Thinking, DeepSeek-R1 distilled models, Granite 4.0 hybrid/MoE
**Parameter Range**: 1-8B parameters

---

## Executive Summary

### Key Findings

1. **Scale Threshold for Native CoT**: Traditional chain-of-thought (CoT) reasoning is an emergent property requiring ~100B parameters. Small models (under 8B) produce illogical reasoning chains without specialized training.

2. **Distillation Breakthrough**: Modern distilled reasoning models (DeepSeek-R1-Distill, Qwen3-4B-Thinking) can achieve 80%+ accuracy on mathematical reasoning benchmarks by learning from larger models' reasoning patterns, drastically outperforming small models trained via reinforcement learning.

3. **Sweet Spot Complexity**: Small reasoning models excel at medium-complexity tasks (2-8 step problems) but face complete accuracy collapse beyond certain complexity thresholds. They show three performance regimes:
   - **Low complexity**: Standard models outperform reasoning models
   - **Medium complexity**: Reasoning models demonstrate clear advantage
   - **High complexity**: Both model types face complete collapse

4. **Verbosity and Loop Problems**: Reasoning models suffer from "overthinking phenomenon" - generating thousands of tokens for simple questions and potentially entering infinite reasoning loops, especially for tasks below or above their complexity sweet spot.

5. **Hybrid Architecture Advantage**: IBM's Granite 4.0 hybrid Mamba-2/Transformer with MoE achieves 70% lower memory requirements and 2x faster inference while maintaining competitive performance, suggesting architectural innovation can rival pure parameter scaling.

---

## 1. Task Categories Where Small Reasoning Models Excel

### 1.1 Mathematical Reasoning (Strong Performance)

**Optimal Complexity Range**: Grade school to high school level (GSM8K, MATH-500)

**Evidence**:
- **DeepSeek-R1-Distill-Qwen-1.5B**: 83.9% on MATH-500 (high school level)
- **Qwen3-4B-Thinking-2507**: 97.0% on MATH-500, 81.3% on AIME25 (improvement from 65.6%)
- **DeepSeek-R1-Distill-Llama-70B**: 94.5% on MATH-500 (best among distilled models)

**Why It Works**:
- Math problems require structured, step-by-step reasoning that aligns perfectly with CoT methodology
- Problems have clear verification criteria (correct/incorrect answers)
- Symbolic manipulation follows logical rules that smaller models can learn through distillation

**Performance Characteristics**:
- GSM8K problems (2-8 steps) are ideal for 1-4B models
- MATH-500 (high school) suitable for 4-8B models
- Beyond Olympiad-level (Omni-MATH), even distilled 70B models struggle

**Limitations**:
- High variance in performance on different instances of same question type
- Fragility with minor difficulty increases
- Sensitivity to inconsequential information (suggests pattern matching rather than true reasoning)

### 1.2 Code Generation (Moderate to Strong Performance)

**Optimal Use Cases**:
- Small functions and utility code
- API calling and structured outputs
- Debugging with clear error messages
- Single-file implementations

**Evidence**:
- **Qwen3-4B-Thinking-2507**: 71.2% on BFCL-v3 (Berkeley Function Calling)
- **DeepSeek-R1-Distill-Qwen-1.5B**: 16.9% on LiveCodeBench (shows limitations)
- **DeepSeek-R1-Distill-Llama-70B**: 57.5% on LiveCodeBench, CodeForces rating 1633

**Why It Works**:
- Structured API outputs align with reasoning model strengths
- Function calling requires step-by-step decomposition
- Single-file code can be verified incrementally

**Limitations**:
- Smallest models (<3B) struggle with programming tasks
- Multi-file refactoring requires more context than small models can handle
- Complex architectural decisions exceed capacity

**Best Practices**:
- Use for quick syntax questions and utility code
- Avoid for large refactoring or architectural planning
- Leverage for structured outputs (JSON, API calls)

### 1.3 Logical Reasoning and Deduction (Variable Performance)

**Optimal Complexity**: Single-step to 3-step logical deductions

**Evidence**:
- **Qwen3-4B**: 65.9 on MLogiQA
- Performance degrades significantly on multi-hop reasoning
- ARC-Challenge shows complete failure after pruning (indicator of fragility)

**Why Mixed Results**:
- Simple logical puzzles work well (clear premises and rules)
- Multi-hop reasoning exceeds working memory capacity
- Abstract symbolic manipulation requires larger parameter counts

**Recommendations**:
- Suitable for single-premise deductions
- Avoid complex multi-step logical chains
- Use structured prompts to guide reasoning

### 1.4 Planning and Multi-Step Reasoning (Limited Performance)

**Performance**: Generally poor, especially for models under 3B

**Evidence**:
- Models under 3B show "negligible changes on MMLU and MathQA"
- Instability suggests lack of intrinsic reasoning ability
- ~3B marks a "capacity threshold" where models can follow multi-step guidance

**Key Insight**:
- Below 3B: Retrieved instructions appear as noise
- At 3B+: Models can leverage structured guidance
- 4-8B: Can handle 4-6 step plans with clear structure

**Recommendations**:
- Break plans into discrete, verifiable steps
- Use external tools for complex planning
- Limit planning depth to 3-5 steps for 4B models

### 1.5 Task Categories to Avoid

1. **Long-Context Reasoning**: While Qwen3-4B supports 262k context, reasoning quality degrades over long sequences
2. **Abstract Symbolic Manipulation**: Requires ~100B parameters for native capability
3. **Open-Ended Creative Tasks**: Reasoning overhead provides no benefit
4. **Multi-Modal Complex Reasoning**: Small models lack capacity for joint vision-language reasoning
5. **Multi-File Codebase Refactoring**: Exceeds context and reasoning capacity

---

## 2. The "Sweet Spot" Problem Complexity

### 2.1 Three Performance Regimes

Research identifies three distinct regimes based on problem complexity:

#### Regime 1: Low Complexity (Simple Tasks)
- **Examples**: Basic arithmetic (2+3), single-step lookups, simple definitions
- **Performance**: Standard (non-reasoning) models OUTPERFORM reasoning models
- **Reason**: Reasoning overhead (verbosity, token budget) costs more than it helps
- **Recommendation**: Use dense models without reasoning for these tasks

#### Regime 2: Medium Complexity (Sweet Spot)
- **Examples**: GSM8K problems (2-8 steps), MATH-500, function calling, structured outputs
- **Performance**: Reasoning models show CLEAR ADVANTAGE over standard models
- **Characteristics**:
  - 2-8 reasoning steps required
  - Clear verification criteria
  - Structured problem space
  - Intermediate steps can be validated
- **Recommendation**: OPTIMAL use case for small reasoning models

#### Regime 3: High Complexity (Beyond Capacity)
- **Examples**: Olympiad math (Omni-MATH), multi-file refactoring, complex architectural decisions
- **Performance**: COMPLETE ACCURACY COLLAPSE for both reasoning and standard models
- **Characteristics**:
  - Reasoning effort increases initially, then DECREASES despite token budget remaining
  - Models may enter infinite loops
  - Verbosity explodes without accuracy gains
- **Recommendation**: Task exceeds model capacity; use larger models or decompose problem

### 2.2 Quantitative Complexity Boundaries

Based on benchmark analysis:

| Model Size | Optimal Step Count | Max Reliable Steps | Benchmark Examples |
|-----------|-------------------|-------------------|-------------------|
| 1.5B | 2-4 steps | 5 steps | GSM8K (simple), basic function calling |
| 3-4B | 3-6 steps | 8 steps | GSM8K (full), MATH-500, BFCL-v3 |
| 7-8B | 4-8 steps | 12 steps | MATH-500, AIME (partial), LiveCodeBench |

**Critical Threshold**: ~3B parameters marks where models gain "intrinsic reasoning ability" to leverage retrieved scaffolds.

### 2.3 Problem Complexity Indicators

**Signs a task is in the sweet spot**:
- Problem has 3-8 clearly definable steps
- Each step can be verified independently
- Total reasoning likely under 2000 tokens
- Domain has structured rules (math, logic, code syntax)
- Similar problems exist in training data

**Signs a task is too simple**:
- Answer can be retrieved directly
- Single-step lookup or calculation
- No intermediate reasoning needed

**Signs a task is too complex**:
- Requires more than 10 reasoning steps
- Steps are interdependent and can't be verified independently
- Problem requires abstract symbolic manipulation
- Domain requires expert human knowledge
- Similar problems likely absent from training data

---

## 3. Chain-of-Thought Length vs Accuracy Correlation

### 3.1 The Verbosity Problem

**Key Finding**: Longer CoT does NOT always mean better accuracy. Small reasoning models suffer from "overthinking phenomenon."

**Evidence**:
- Simple questions like "What is 2 plus 3?" can generate thousands of reasoning tokens in smaller models
- Extreme cases enter infinite reasoning loops, exhausting token budgets
- Reasoning effort scales with problem complexity UP TO A THRESHOLD, then declines

### 3.2 Optimal Reasoning Length by Task Complexity

Based on research findings:

| Task Complexity | Optimal Reasoning Tokens | Performance Impact |
|----------------|-------------------------|-------------------|
| Simple (1-2 steps) | 0-100 tokens | Reasoning HURTS performance |
| Medium (3-6 steps) | 100-500 tokens | Reasoning HELPS significantly |
| Complex (7-10 steps) | 500-2000 tokens | Reasoning helps if within capacity |
| Very Complex (10+ steps) | 2000+ tokens | Accuracy collapse, verbosity explosion |

**Qwen3-4B-Thinking-2507 Configuration**:
- Highly challenging tasks: 81,920 token output limit
- Other tasks: 32,768 token output limit
- This suggests even 4B models expect long reasoning chains for complex problems

### 3.3 Accuracy Correlation Patterns

**Positive Correlation Zone** (Medium Complexity):
- More reasoning tokens = higher accuracy
- Sweet spot: 200-800 tokens for 4B models
- Reasoning provides scaffold for step-by-step solution

**Negative Correlation Zone** (High Complexity):
- Beyond threshold, reasoning effort DECREASES despite token budget
- Model "gives up" on problem
- Verbosity without accuracy improvement

**Zero Correlation Zone** (Low Complexity):
- Reasoning tokens wasted on trivial problems
- Direct answer more efficient

### 3.4 Strategies to Optimize CoT Length

**Early Stopping Methods**:
1. **Answer Convergence**: Stop when model's predicted answer stabilizes
2. **Step Pruner**: RL framework that penalizes redundant reasoning steps
3. **Learn-to-Stop**: Unsupervised training to detect completion

**Compression Techniques**:
1. **LightThinker**: Compress verbose reasoning into "gist tokens"
2. **Pruning intermediate steps**: Keep only essential reasoning
3. **Distillation**: Learn compressed reasoning from larger models

**Practical Recommendations**:
- Set max_tokens based on expected problem complexity
- Monitor reasoning_content length in responses
- If reasoning exceeds 2000 tokens for 4B model, task may be too complex
- Use temperature=0.0 to reduce verbosity
- Implement early stopping if answer repeats

---

## 4. Dense vs Hybrid/MoE Model Performance

### 4.1 Architecture Comparison

#### Dense Models (e.g., Granite-micro-3b)
- **Advantages**:
  - Simpler architecture, easier to optimize
  - Consistent performance across tasks
  - Lower latency for short sequences
  - Better for simple, low-complexity tasks

- **Disadvantages**:
  - Higher memory requirements
  - All parameters active for every token
  - Less efficient for varied task complexity

#### Hybrid Models (e.g., Granite-h-micro-3b, Granite-tiny-7b)
- **Architecture**: Mamba-2/Transformer hybrid (9:1 ratio)
- **Advantages**:
  - 70% lower memory requirements
  - 2x faster inference speeds
  - Better long-context handling
  - Suitable for multi-session inference

- **Disadvantages**:
  - More complex architecture
  - Potential for uneven performance across task types

#### MoE Models (e.g., Qwen3-30B-A3B, Granite-4.0-H-Tiny/Small)
- **Architecture**: Mixture of Experts with selective activation
- **Advantages**:
  - Granite-4.0-H-Tiny: 7B total, ~1B active per token
  - Granite-4.0-H-Small: 32B total, ~9B active per token
  - Best performance-to-resource ratio
  - Shared experts improve parameter efficiency

- **Disadvantages**:
  - Most complex architecture
  - May have performance variance across domains

### 4.2 Performance Comparison by Task Type

#### Mathematical Reasoning
**Winner: Hybrid/MoE models**
- Granite-4.0-H-Small outperforms larger dense models
- Qwen3-4B-Thinking (dense) achieves 97.0% on MATH-500
- MoE models can match 70B dense performance with 9B active parameters

**Reasoning**: Math benefits from specialized expert activation

#### Code Generation
**Winner: Varies by complexity**
- Simple code: Dense models (lower latency)
- Structured outputs: Hybrid models (better at following constraints)
- Complex multi-file: MoE models (can activate relevant experts)

**Evidence**:
- Qwen3-30B-A3B-Instruct-2507 (MoE): Competitive with much larger dense models
- Granite-4.0-H-Small: Top-tier function calling performance

#### Long-Context Tasks
**Winner: Hybrid models**
- Qwen3-4B: 85.2 average RULER score with 131k context
- Granite 4.0 hybrid: 70% lower memory for long-context inference
- Dense models struggle with memory requirements

**Reasoning**: Mamba-2 layers handle long sequences more efficiently than pure transformers

#### Short, Simple Tasks
**Winner: Dense models**
- Lower overhead, faster response
- Consistent performance without expert routing
- Better for tasks in Regime 1 (low complexity)

### 4.3 Resource Efficiency Comparison

| Model Type | Total Params | Active Params | Memory | Inference Speed | Best Use Case |
|-----------|-------------|---------------|---------|-----------------|---------------|
| Dense 3B | 3B | 3B | 100% | 1x | Simple tasks, short context |
| Hybrid 3B | 3B | 3B | 30% | 2x | Long context, varied tasks |
| MoE 7B (~1B active) | 7B | 1B | 15% | 3x | Complex reasoning, low resources |
| MoE 32B (~9B active) | 32B | 9B | 30% | 1.5x | Maximum performance, efficiency |

**Key Insights**:
1. Hybrid models offer best memory efficiency for equivalent parameter count
2. MoE models provide best performance-to-active-parameter ratio
3. Dense models still competitive for tasks under 3-step complexity
4. Quantization (Q8, Q6, Q4) has minimal impact on reasoning performance

### 4.4 When to Choose Each Architecture

**Choose Dense Models When**:
- Tasks are consistently simple (Regime 1)
- Latency is critical
- Context length under 4k tokens
- Deployment environment has sufficient memory
- Example: qwen3-4b-thinking for focused math problems

**Choose Hybrid Models When**:
- Memory is constrained
- Long context required (32k+ tokens)
- Multi-session inference needed
- Balanced performance across task types desired
- Example: granite-h-micro-3b for general assistant

**Choose MoE Models When**:
- Maximum performance per active parameter needed
- Task complexity varies significantly
- Domain expertise required (experts specialize)
- Willing to accept slightly higher architectural complexity
- Example: granite-h-micro-3b for varied enterprise workloads

---

## 5. Designed Test Cases for API Validation

### 5.1 Mathematical Reasoning Tests

#### Test 1: Simple Arithmetic (Regime 1 - Below Sweet Spot)
```bash
curl -sk https://llama.krohnos.io/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen3-4b-thinking-2507-q8_0",
    "messages": [{"role": "user", "content": "What is 15 + 27?"}],
    "max_tokens": 500,
    "temperature": 0.0
  }'
```
**Expected**: Short reasoning (~50-100 tokens), correct answer (42)
**Hypothesis**: Dense model may be faster, reasoning provides little value

#### Test 2: Grade School Math (Regime 2 - Sweet Spot)
```bash
curl -sk https://llama.krohnos.io/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen3-4b-thinking-2507-q8_0",
    "messages": [{"role": "user", "content": "A bakery sells 3 types of cookies: chocolate chip for $2, oatmeal for $1.50, and sugar cookies for $1. If someone buys 4 chocolate chip, 2 oatmeal, and 5 sugar cookies, what is the total cost?"}],
    "max_tokens": 1000,
    "temperature": 0.0
  }'
```
**Expected**: Medium reasoning (200-500 tokens), correct answer ($16)
**Hypothesis**: Reasoning model shows clear advantage, 3-4 step solution

#### Test 3: Multi-Step Word Problem (Regime 2 Upper Bound)
```bash
curl -sk https://llama.krohnos.io/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen3-4b-thinking-2507-q8_0",
    "messages": [{"role": "user", "content": "A car travels at 60 mph for 2.5 hours, then slows to 45 mph for the next 90 minutes. After a 30-minute break, it travels at 55 mph for 1.5 hours. What is the total distance covered?"}],
    "max_tokens": 1500,
    "temperature": 0.0
  }'
```
**Expected**: Longer reasoning (400-800 tokens), correct answer (282.5 miles)
**Hypothesis**: Approaching complexity limit for 4B model, may show verbosity

#### Test 4: Compare Granite Dense vs Hybrid on Same Problem
```bash
# Dense model
curl -sk https://llama.krohnos.io/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "granite-micro-3b-q6_k",
    "messages": [{"role": "user", "content": "If a rectangle has a length of 12 cm and a width of 7 cm, what is its area and perimeter?"}],
    "max_tokens": 800,
    "temperature": 0.0
  }'

# Hybrid model
curl -sk https://llama.krohnos.io/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "granite-h-micro-3b-q6_k",
    "messages": [{"role": "user", "content": "If a rectangle has a length of 12 cm and a width of 7 cm, what is its area and perimeter?"}],
    "max_tokens": 800,
    "temperature": 0.0
  }'
```
**Expected**: Both correct (area=84 cm², perimeter=38 cm)
**Hypothesis**: Similar accuracy, hybrid may use fewer tokens

### 5.2 Logical Reasoning Tests

#### Test 5: Simple Deduction (Regime 2)
```bash
curl -sk https://llama.krohnos.io/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen3-4b-thinking-2507-q8_0",
    "messages": [{"role": "user", "content": "All cats are mammals. All mammals are animals. Therefore, are all cats animals? Explain your reasoning step by step."}],
    "max_tokens": 1000,
    "temperature": 0.0
  }'
```
**Expected**: Clear step-by-step deduction (200-400 tokens), correct (yes)
**Hypothesis**: Reasoning model excels at structured logical deduction

#### Test 6: Multi-Hop Reasoning (Regime 2/3 Boundary)
```bash
curl -sk https://llama.krohnos.io/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen3-4b-thinking-2507-q8_0",
    "messages": [{"role": "user", "content": "Five people (A, B, C, D, E) are in a line. A is before B. C is before D. E is before A. D is before B. What is the order from first to last?"}],
    "max_tokens": 1500,
    "temperature": 0.0
  }'
```
**Expected**: Medium-long reasoning (500-1000 tokens), correct order (E, C/A, A/C, D, B)
**Hypothesis**: May struggle with constraint satisfaction, possible verbosity

### 5.3 Code Generation Tests

#### Test 7: Simple Function (Regime 2)
```bash
curl -sk https://llama.krohnos.io/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen3-4b-thinking-2507-q8_0",
    "messages": [{"role": "user", "content": "Write a Python function that takes a list of numbers and returns the sum of even numbers only. Include example usage."}],
    "max_tokens": 1000,
    "temperature": 0.0
  }'
```
**Expected**: Clear implementation (300-600 tokens), working code
**Hypothesis**: Reasoning helps structure solution, good performance expected

#### Test 8: Structured Output (Regime 2)
```bash
curl -sk https://llama.krohnos.io/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen3-4b-thinking-2507-q8_0",
    "messages": [{"role": "user", "content": "Generate a JSON object for a user profile with fields: name, age, email, and a nested address object with street, city, and zipcode. Use realistic example data."}],
    "max_tokens": 800,
    "temperature": 0.0
  }'
```
**Expected**: Valid JSON (200-400 tokens), proper nesting
**Hypothesis**: Reasoning models excel at structured outputs per research

#### Test 9: Compare Granite Models on Code
```bash
# Hybrid tiny (7B, ~1B active)
curl -sk https://llama.krohnos.io/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "granite-tiny-7b-q4_k_m",
    "messages": [{"role": "user", "content": "Write a function to check if a string is a palindrome. Handle edge cases."}],
    "max_tokens": 800,
    "temperature": 0.0
  }'

# Dense micro (3B)
curl -sk https://llama.krohnos.io/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "granite-micro-3b-q6_k",
    "messages": [{"role": "user", "content": "Write a function to check if a string is a palindrome. Handle edge cases."}],
    "max_tokens": 800,
    "temperature": 0.0
  }'
```
**Hypothesis**: 7B MoE may produce more robust code despite fewer active parameters

### 5.4 Planning and Multi-Step Tests

#### Test 10: Simple Planning (Regime 2)
```bash
curl -sk https://llama.krohnos.io/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen3-4b-thinking-2507-q8_0",
    "messages": [{"role": "user", "content": "Create a 4-step plan to organize a small birthday party for 10 people. Each step should be specific and actionable."}],
    "max_tokens": 1200,
    "temperature": 0.0
  }'
```
**Expected**: Clear 4-step plan (400-700 tokens), actionable items
**Hypothesis**: Within capacity, should perform well

#### Test 11: Complex Planning (Regime 3 - Beyond Capacity)
```bash
curl -sk https://llama.krohnos.io/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen3-4b-thinking-2507-q8_0",
    "messages": [{"role": "user", "content": "Design a comprehensive 15-step project plan for building a mobile app from scratch, including research, design, development, testing, deployment, and maintenance phases. Consider dependencies between steps."}],
    "max_tokens": 3000,
    "temperature": 0.0
  }'
```
**Expected**: May show verbosity or incomplete reasoning (1500+ tokens)
**Hypothesis**: Exceeds capacity, possible accuracy degradation or loops

### 5.5 Complexity Boundary Tests

#### Test 12: Token Usage Correlation
Run Tests 1, 2, 3 in sequence and analyze:
```bash
# Parse each response to extract:
# - reasoning_content length
# - content length
# - total_tokens
# - accuracy (correct/incorrect)
# - response_time

# Expected pattern:
# Test 1 (simple): Low tokens, correct, reasoning unnecessary
# Test 2 (medium): Medium tokens, correct, reasoning helpful
# Test 3 (complex): High tokens, correct but verbose, approaching limit
```

#### Test 13: Verbosity Detection
```bash
curl -sk https://llama.krohnos.io/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen3-4b-thinking-2507-q8_0",
    "messages": [{"role": "user", "content": "What is 2 plus 3?"}],
    "max_tokens": 2000,
    "temperature": 0.0
  }'
```
**Expected**: Minimal reasoning (<100 tokens) for trivial question
**Hypothesis**: Well-trained models avoid overthinking simple problems

### 5.6 Test Analysis Framework

For each test, record:
1. **Model**: Which model was tested
2. **Task Complexity**: Regime 1, 2, or 3
3. **Reasoning Length**: Character count of reasoning_content
4. **Answer Length**: Character count of content
5. **Total Tokens**: From usage field
6. **Accuracy**: Correct/Incorrect/Partial
7. **Response Time**: Latency measurement
8. **Verbosity Score**: Reasoning tokens / minimal expected tokens

**Analysis Questions**:
- At what complexity do reasoning models outperform dense models?
- What is the token usage inflection point where accuracy degrades?
- Do hybrid models maintain accuracy with fewer tokens?
- What problem types show infinite loop tendencies?
- How does quantization level (q8 vs q6 vs q4) affect reasoning quality?

---

## 6. Key Research Insights and Recommendations

### 6.1 Critical Findings Summary

1. **3B Parameter Threshold**: Below 3B, models lack intrinsic reasoning ability. At 3B+, they can leverage structured guidance effectively.

2. **Distillation > RL for Small Models**: Distilled models (learning from larger model reasoning) vastly outperform RL-trained small models on reasoning tasks.

3. **Medium Complexity Sweet Spot**: Small reasoning models excel at 3-8 step problems with clear verification criteria. Performance collapses outside this range.

4. **Verbosity is a Feature and a Bug**: Longer reasoning helps in the sweet spot but becomes a liability for simple tasks and creates infinite loops for complex tasks.

5. **Architecture Matters**: Hybrid Mamba-2/Transformer and MoE architectures can match or exceed dense model performance with 70% lower memory and 2x speed improvements.

6. **Task-Specific Performance**: Mathematical reasoning > Structured outputs > Simple code > Logical deduction > Complex planning

### 6.2 Practical Recommendations

#### For Model Selection:

**Use 1.5-3B Reasoning Models For**:
- Grade school math (GSM8K level)
- Simple function calling
- 2-4 step logical deductions
- Single-file code snippets
- Structured data generation (JSON, XML)

**Use 4-7B Reasoning Models For**:
- High school math (MATH-500)
- Multi-step word problems
- API integration and tool calling
- Code with 3-5 functions
- 4-8 step planning tasks
- Debugging with clear error messages

**Use 7B+ MoE/Hybrid Models For**:
- Varied task complexity
- Long-context inference
- Multi-domain applications
- Resource-constrained environments needing maximum performance
- Production systems requiring efficiency

**Avoid Small Reasoning Models For**:
- Simple lookups or single-step calculations (use dense models)
- Olympiad-level mathematics (use 70B+ models)
- Multi-file codebase refactoring (use coding-specific large models)
- Open-ended creative writing (reasoning overhead counterproductive)
- Tasks requiring abstract symbolic manipulation

#### For Deployment:

1. **Set Appropriate Token Limits**:
   - Simple tasks: 500 max_tokens
   - Medium tasks: 1000-1500 max_tokens
   - Complex tasks: 2000-3000 max_tokens (but verify model capacity)

2. **Implement Early Stopping**:
   - Monitor reasoning_content length
   - Stop if answer converges (repeated outputs)
   - Alert if reasoning exceeds expected length for task complexity

3. **Temperature Settings**:
   - Use temperature=0.0 for mathematical and logical tasks
   - Use temperature=0.1-0.3 for code generation
   - Avoid high temperatures for reasoning tasks (increases verbosity)

4. **Quantization Strategy**:
   - Q8 for maximum accuracy (minimal degradation)
   - Q6 for balanced performance/size
   - Q4 for resource-constrained (minimal reasoning impact per research)
   - Avoid aggressive pruning (causes 32% accuracy drop)

5. **Hybrid Architecture Preference**:
   - Default to hybrid/MoE for production (better efficiency)
   - Use dense only when latency is critical and resources available
   - Consider long-context needs (hybrid excels here)

### 6.3 Future Research Directions

1. **Optimal Reasoning Length Prediction**: Develop models that predict required reasoning tokens based on problem analysis before full inference.

2. **Dynamic Expert Routing**: Improve MoE models to route to specialized experts based on problem type detection.

3. **Hybrid Dense-Reasoning Architectures**: Explore models that switch between dense and reasoning modes based on detected complexity.

4. **Better Complexity Detection**: Create automated systems to classify problems into Regimes 1, 2, or 3 before inference.

5. **Reasoning Compression Techniques**: Advance "gist token" and other compression methods to reduce verbosity while maintaining accuracy.

6. **Task-Specific Distillation**: Create distilled models specialized for domains (math-only, code-only, logic-only) to achieve better performance at smaller sizes.

---

## 7. Sources and References

### Academic Papers and Research

1. [Towards Reasoning Ability of Small Language Models](https://arxiv.org/html/2502.11569v3) - Analysis of reasoning capabilities in small models
2. [Chain-of-Thought Prompting Elicits Reasoning in Large Language Models](https://arxiv.org/abs/2201.11903) - Original CoT research
3. [Enhancing Generalization in Chain of Thought Reasoning for Smaller Models](https://arxiv.org/abs/2501.09804) - Methods for improving small model reasoning
4. [The Illusion of Thinking: Understanding the Strengths and Limitations of Reasoning Models](https://arxiv.org/abs/2506.06941) - Problem complexity analysis
5. [Beyond Token Length: Step Pruner for Efficient Reasoning](https://arxiv.org/html/2510.03805) - Verbosity solutions
6. [Answer Convergence as a Signal for Early Stopping](https://arxiv.org/html/2506.02536v1) - Early stopping methods
7. [Stop Overthinking: A Survey on Efficient Reasoning for LLMs](https://arxiv.org/html/2503.16419v4) - Overthinking phenomenon
8. [Enhancing Code Generation Performance via Distillation](https://arxiv.org/html/2403.13271v1) - Code generation improvements

### Model Documentation and Benchmarks

9. [Qwen3 Technical Report](https://arxiv.org/pdf/2505.09388) - Official Qwen3 research
10. [Qwen/Qwen3-4B-Thinking-2507 on Hugging Face](https://huggingface.co/Qwen/Qwen3-4B-Thinking-2507) - Model card and benchmarks
11. [Qwen3: Think Deeper, Act Faster](https://qwenlm.github.io/blog/qwen3/) - Official blog post
12. [DeepSeek-R1 on Hugging Face](https://huggingface.co/deepseek-ai/DeepSeek-R1) - Model documentation
13. [GitHub - DeepSeek-R1](https://github.com/deepseek-ai/DeepSeek-R1) - Official repository
14. [Re-Distilling Smaller DeepSeek R1 Models](https://mobiusml.github.io/r1_redistill_blogpost/) - Distillation techniques
15. [IBM Granite 4.0 Announcement](https://www.ibm.com/new/announcements/ibm-granite-4-0-hyper-efficient-high-performance-hybrid-models) - Hybrid architecture details
16. [Hybrid Thinking: Inside Granite 4.0 Architecture](https://www.ibm.com/think/news/hybrid-thinking-inside-architecture-granite-4-0) - Technical deep dive

### Industry Analysis and Benchmarks

17. [Top 15 Small Language Models for 2025 - DataCamp](https://www.datacamp.com/blog/top-small-language-models) - SLM landscape
18. [Best Small Language Models - Benchmark Results](https://medium.com/@darrenoberst/best-small-language-models-for-accuracy-and-enterprise-use-cases-benchmark-results-cf71964759c8) - Performance comparisons
19. [Benchmarking Reasoning Models - ROCm Blogs](https://rocm.blogs.amd.com/artificial-intelligence/benchmark-reasoning-models/README.html) - Reasoning model benchmarks
20. [Ultimate Guide - Best Small LLMs Under 10B Parameters](https://www.siliconflow.com/articles/en/best-small-LLMs-under-10B-parameters) - Comprehensive guide
21. [GSM8K-Platinum: Performance Gaps in Frontier LLMs](https://gradientscience.org/gsm8k-platinum/) - Math reasoning evaluation
22. [GSM8K & MATH Benchmarks](https://verityai.co/blog/gsm8k-math-benchmarks-mathematical-reasoning) - Mathematical reasoning analysis
23. [General-Purpose vs Reasoning Models - Azure OpenAI](https://techcommunity.microsoft.com/blog/azure-ai-foundry-blog/general-purpose-vs-reasoning-models-in-azure-openai/4403091) - Model comparison guide
24. [Comparing AI Models for Code Generation](https://graphite.com/guides/ai-coding-model-comparison) - Code generation analysis

### Additional Resources

25. [DeepSeek-R1: Features and Comparisons - DataCamp](https://www.datacamp.com/blog/deepseek-r1) - Model overview
26. [Qwen3: Features and Comparisons - DataCamp](https://www.datacamp.com/blog/qwen3) - Model overview
27. [Language Models Perform Reasoning via Chain of Thought - Google Research](https://research.google/blog/language-models-perform-reasoning-via-chain-of-thought/) - CoT introduction
28. [Granite 4.0 Nano: How Small Can You Go?](https://huggingface.co/blog/ibm-granite/granite-4-nano) - Extreme small models
29. [Everything About Reasoning Models - Microsoft](https://techcommunity.microsoft.com/blog/azure-ai-foundry-blog/everything-you-need-to-know-about-reasoning-models-o1-o3-o4-mini-and-beyond/4406846) - Reasoning model landscape

---

## Appendix: Test Execution Guide

### A.1 Prerequisites

- Access to https://llama.krohnos.io API
- curl installed
- jq for JSON parsing (optional but recommended)
- Python 3 for data analysis (optional)

### A.2 Running Test Suite

```bash
#!/bin/bash
# Save as run_reasoning_tests.sh

ENDPOINT="https://llama.krohnos.io/v1/chat/completions"
MODELS=("qwen3-4b-thinking-2507-q8_0" "granite-micro-3b-q6_k" "granite-h-micro-3b-q6_k" "granite-tiny-7b-q4_k_m")

# Test simple math across all models
for model in "${MODELS[@]}"; do
  echo "Testing $model on simple math..."
  curl -sk $ENDPOINT \
    -H "Content-Type: application/json" \
    -d "{
      \"model\": \"$model\",
      \"messages\": [{\"role\": \"user\", \"content\": \"What is 15 + 27?\"}],
      \"max_tokens\": 500,
      \"temperature\": 0.0
    }" | jq '.' > "results/${model}_simple_math.json"
done

# Add more test iterations as needed
```

### A.3 Analysis Script

```python
#!/usr/bin/env python3
# Save as analyze_results.py

import json
import glob
from pathlib import Path

results = []
for file in glob.glob("results/*.json"):
    with open(file) as f:
        data = json.load(f)
        model = Path(file).stem.rsplit('_', 2)[0]
        msg = data['choices'][0]['message']

        results.append({
            'model': model,
            'reasoning_tokens': len(msg.get('reasoning_content', '')),
            'answer_tokens': len(msg.get('content', '')),
            'total_tokens': data['usage']['total_tokens'],
            'answer': msg['content']
        })

# Analyze patterns
for r in results:
    print(f"{r['model']}: {r['total_tokens']} tokens, reasoning: {r['reasoning_tokens']}")
```

### A.4 Expected Outputs

Document findings in `/home/moot/crucible/tree/research/cli-tool-generation/docs/test-results/` with:
- Raw JSON responses
- Summary statistics
- Accuracy measurements
- Token usage analysis
- Performance comparisons

---

**Document Status**: Literature review complete, test cases designed, awaiting manual API testing execution.

**Next Steps**:
1. Execute test suite against https://llama.krohnos.io API
2. Analyze reasoning_content vs content ratios
3. Identify specific complexity thresholds for each model
4. Document verbosity patterns and loop conditions
5. Create final recommendations for CLI tool generation use case
