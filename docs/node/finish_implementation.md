MCP Host Project Overview
Project Purpose
MCP Host is a Rust application designed to create a framework for AI agent interactions with various tools and services. The project's primary purpose is to provide a unified interface for LLM (Large Language Model) agents to interact with external tools and services through a defined protocol.
Key features include:

A client-server architecture where the host connects to various tool servers
Support for multiple AI providers (DeepSeek, OpenAI, Anthropic, etc.)
A conversation system that allows LLMs to call tools, process results, and generate final responses
Verification of responses against generated criteria to ensure quality
An interactive REPL (Read-Eval-Print Loop) for user interaction
An evaluation framework for benchmarking different LLM configurations

Main Components

MCPHost: The central coordinator that manages connections to tool servers and AI providers.
Conversation System:

ConversationState: Maintains the history and context of a conversation
ConversationLogic: Handles the flow of conversation, including tool calls and verification
ConversationService: Provides helper functions for the conversation system


AI Client Layer:

AIClient: Abstract interface for different AI providers
RLLMAdapter: Adapter for the rllm library (Rust LLM wrapper)
OpenRouterAdapter: Support for the OpenRouter API


Tool System:

ToolParser: Parses tool calls from LLM responses
ManagedServer: Manages connections to external tool servers


REPL Interface:

Interactive console for user interaction
Command processing
Chat history management


Evaluation System:

mcp_eval: A binary for running evaluations across different LLM providers
Test case definition and execution
Performance grading



Significant LLM Prompts
The project relies on several critical prompts that drive its functionality:
1. Tool System Prompt
This is the core prompt that instructs the LLM on how to use tools. It's generated by the generate_tool_system_prompt function in conversation_service.rs:
You are a helpful assistant with access to tools. Use tools EXACTLY according to their descriptions and required format.

**Core Instructions for Tool Use:**

1.  **Address the Full Request:** Plan and execute all necessary steps sequentially using tools. If generating information *and* performing an action (like saving), **include the key information/summary in your response** along with action confirmation.
2.  **Execution Model & Reacting to Results:**
    *   **Dispatch:** All tools you call in a single response turn are dispatched *before* you receive results for *any* of them.
    *   **Results:** You *will* receive the results for all dispatched tools in the *next* conversation turn.
    *   **No Same-Turn Chaining:** Because of the dispatch timing, **you cannot use the result of one tool as input for another tool within the *same* response turn.** Plan sequential, dependent calls across multiple turns.
    *   **Verification & Adaptation:** Carefully review tool results when you receive them. Verify success/failure, extract data, and **change your plan or response if the results require it.**
3.  **Be Truthful & Cautious:** Only confirm actions (e.g., "file saved") if the tool result explicitly confirms success. Report errors. Be careful with tools that modify external systems.
4.  **Use Correct Format:** Use the precise `<<<TOOL_CALL>>>...<<<END_TOOL_CALL>>>` format with valid JSON (`name`, `arguments`) for all tool calls.

# Tool Descriptions...
{tools_info}

When you need to use a tool, you MUST format your request exactly as follows, including the delimiters:
<<<TOOL_CALL>>>
{
  "name": "tool_name",
  "arguments": {
    "arg1": "value1",
    "arg2": "value2"
  }
}
<<<END_TOOL_CALL>>>

Important:
- Only include ONE tool call JSON block per delimiter section. Use multiple sections for multiple parallel calls in one turn.
- You can include explanatory text before or after the tool call block.
- If no tool is needed, just respond normally.
2. Verification Criteria Generation Prompt
Used to generate evaluation criteria for a user's request in conversation_logic.rs:
Based on the following user request, list concise, verifiable criteria for a successful response. 
Focus on key actions, information requested, and constraints mentioned. 
Output ONLY the criteria list, one criterion per line, starting with '- '. Do not include any other text.

User Request:
{user_request}

Criteria:
3. Response Verification Prompt
Used to verify a response against criteria in conversation_logic.rs:
You are a strict evaluator. Verify if the 'Relevant Conversation Sequence' below meets ALL the 'Success Criteria' based on the 'Original User Request'.

Original User Request:
{original_request}

Success Criteria:
{criteria}

Relevant Conversation Sequence (User messages, Assistant actions/responses, Tool results):
{relevant_history_sequence}

Instructions:
1. Carefully review the *entire sequence* including user feedback, assistant actions (tool calls/results shown), and the final response.
2. Compare this sequence against each point in the 'Success Criteria'.
3. Determine if the *outcome* of the assistant's actions and the final response *fully and accurately* satisfy *all* criteria.
4. Output ONLY the raw JSON object. Your entire response must start with `{` and end with `}`.
5. The JSON object must have the following structure: `{"passes": boolean, "feedback": "string (provide concise feedback ONLY if passes is false, explaining which criteria failed and why, referencing the assistant's actions/responses if relevant)"}`
6. ABSOLUTELY DO NOT include any other text, explanations, apologies, introductory phrases, or markdown formatting like ```json or ```.
4. Tool Results Processing Prompt
Used after a tool has been executed to help the LLM process the results appropriately:
You have received results from the tool(s) you called previously (shown immediately above).
Analyze these results carefully.
Based *only* on these results and the original user request:
1. If the results provide the necessary information to fully answer the user's original request, formulate and provide the final answer now. Do NOT call any more tools unless absolutely necessary for clarification based *specifically* on the results received.
2. If the results are insufficient or indicate an error, decide if another *different* tool call is needed to achieve the original goal. If so, call the tool using the <<<TOOL_CALL>>>...<<<END_TOOL_CALL>>> format.
3. If you cannot proceed further, explain why.
5. Invalid Tool Format Feedback Prompt
Used when the LLM attempts to make a tool call but uses an incorrect format:
Correction Request:
You attempted to call a tool, but the format was incorrect. 
Remember to use the exact format including delimiters and a valid JSON object with 'name' (string) and 'arguments' (object) fields.

Your invalid attempt contained:
{invalid_content}

Please correct the format and try the tool call again, or provide a text response if you no longer need the tool.
6. Verification Failure Feedback Prompt
Used when a response fails verification, to guide the LLM toward fixing issues:
Correction Request:
Your previous response failed verification.
Feedback: {feedback}

Please analyze this feedback carefully and revise your plan and response to fully address the original request and meet all success criteria. 
You may need to use tools differently or provide more detailed information.
7. Conversation Compaction Prompt
Used to summarize a conversation when it gets too long:
You are an expert conversation summarizer. Analyze the following conversation history and provide a concise summary. Focus on:
- Key user requests and goals.
- Important information discovered or generated.
- Decisions made.
- Final outcomes or current status.
- Any critical unresolved questions or next steps mentioned.

Keep the summary factual and brief, retaining essential context for the conversation to continue.

Conversation History:
{history_string}

Concise Summary:
Architecture and Flow
The system works as follows:

The MCPHost connects to one or more tool servers and initializes an AI client
When a user sends a message, it's added to the conversation state
The LLM generates a response, which may include tool calls
Tool calls are parsed and executed against the appropriate servers
Results are fed back to the LLM, which may make additional tool calls or provide a final response
(Optional) The response is verified against generated criteria
The conversation continues with the next user message

The project provides both an interactive REPL for direct usage and an evaluation framework for benchmarking different LLM configurations against defined tasks.
Conclusion
MCP Host is a sophisticated framework for enabling LLM-powered agents to interact with external tools and services. It provides a robust architecture for capturing user intents, translating them into tool operations, and ensuring high-quality responses through verification. The project is designed to be extensible, supporting multiple AI providers and allowing for easy integration of new tool servers.