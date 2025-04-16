import type { Tool } from '@modelcontextprotocol/sdk/types.js';

export class PromptFactory {

    // --- Tool Related Prompts ---

    public static readonly TOOL_RESULTS_PROMPT = `You have received results from the tool(s) you called previously (shown immediately above).
Analyze these results carefully.
Based *only* on these results and the original user request:
1. If the results provide the necessary information to fully answer the user's original request, formulate and provide the final answer now. Do NOT call any more tools unless absolutely necessary for clarification based *specifically* on the results received.
2. If the results are insufficient or indicate an error, decide if another *different* tool call is needed to achieve the original goal. If so, call the tool using the <<<TOOL_CALL>>>...<<<END_TOOL_CALL>>> format.
3. If you cannot proceed further, explain why.`;

    public static readonly INVALID_TOOL_FORMAT_PROMPT = `Correction Request:
You attempted to call a tool, but the format was incorrect.
Remember to use the exact format including delimiters and a valid JSON object with 'name' (string) and 'arguments' (object) fields.

Your invalid attempt contained:
{invalid_content}

Please correct the format and try the tool call again, or provide a text response if you no longer need the tool.`;

    /**
     * Generates the system prompt including tool definitions.
     * @param allTools Array of available MCP Tools.
     * @returns The generated system prompt string.
     */
    public static createToolSystemPrompt(allTools: Tool[]): string {
        // Basic prompt structure derived from finish_implementation.md
        let prompt = `You are a helpful assistant with access to tools. Use tools EXACTLY according to their descriptions and required format.

**Core Instructions for Tool Use:**

1.  **Address the Full Request:** Plan and execute all necessary steps sequentially using tools. If generating information *and* performing an action (like saving), **include the key information/summary in your response** along with action confirmation.
2.  **Execution Model & Reacting to Results:**
    *   **Dispatch:** All tools you call in a single response turn are dispatched *before* you receive results for *any* of them.
    *   **Results:** You *will* receive the results for all dispatched tools in the *next* conversation turn.
    *   **No Same-Turn Chaining:** Because of the dispatch timing, **you cannot use the result of one tool as input for another tool within the *same* response turn.** Plan sequential, dependent calls across multiple turns.
    *   **Verification & Adaptation:** Carefully review tool results when you receive them. Verify success/failure, extract data, and **change your plan or response if the results require it.**
3.  **Be Truthful & Cautious:** Only confirm actions (e.g., "file saved") if the tool result explicitly confirms success. Report errors. Be careful with tools that modify external systems.
4.  **Use Correct Format:** Use the precise \`<<<TOOL_CALL>>>...<<<END_TOOL_CALL>>>\` format with valid JSON (\`name\`, \`arguments\`) for all tool calls.

# Tool Descriptions
`;

        // Add tool descriptions
        if (allTools.length === 0) {
            prompt += "No tools are currently available.\n";
        } else {
            for (const tool of allTools) {
                prompt += `## ${tool.name}\n`;
                prompt += `${tool.description || 'No description provided.'}\n\n`;

                // Add schema information if available
                if (tool.input_schema) {
                    prompt += "**Arguments Schema:**\n```json\n";
                    try {
                        // Attempt to pretty-print if it's a JSON string or object
                        const schemaObj = typeof tool.input_schema === 'string'
                            ? JSON.parse(tool.input_schema)
                            : tool.input_schema;
                        prompt += JSON.stringify(schemaObj, null, 2);
                    } catch (e) {
                        // Fallback to string representation if parsing fails
                        prompt += String(tool.input_schema);
                    }
                    prompt += "\n```\n\n";
                }
            }
        }

        // Add tool usage format section
        prompt += `
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
- If no tool is needed, just respond normally.`;

        return prompt;
    }

    // --- Verification Related Prompts ---

    public static readonly VERIFICATION_CRITERIA_PROMPT = `Based on the following user request, list concise, verifiable criteria for a successful response.
Focus on key actions, information requested, and constraints mentioned.
Output ONLY the criteria list, one criterion per line, starting with '- '. Do not include any other text.

User Request:
{user_request}

Criteria:`;

    public static readonly VERIFICATION_PROMPT = `You are a strict evaluator. Verify if the 'Relevant Conversation Sequence' below meets ALL the 'Success Criteria' based on the 'Original User Request'.

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
4. Output ONLY the raw JSON object. Your entire response must start with \`{\` and end with \`}\`.
5. The JSON object must have the following structure: \`{"passes": boolean, "feedback": "string (provide concise feedback ONLY if passes is false, explaining which criteria failed and why, referencing the assistant's actions/responses if relevant)"}\`
6. ABSOLUTELY DO NOT include any other text, explanations, apologies, introductory phrases, or markdown formatting like \`\`\`json or \`\`\`.`;

    public static readonly VERIFICATION_FAILURE_PROMPT = `Your previous response failed verification based on the following feedback:
{feedback}

Revise your response to fully address the original request and meet all success criteria based on this feedback. Provide only the corrected response.`;

    // --- Compaction Related Prompt ---

    public static readonly CONVERSATION_COMPACTION_PROMPT = `You are an expert conversation summarizer. Analyze the following conversation history and provide a concise summary. Focus on:
- Key user requests and goals.
- Important information discovered or generated.
- Decisions made.
- Final outcomes or current status.
- Any critical unresolved questions or next steps mentioned.

Keep the summary factual and brief, retaining essential context for the conversation to continue.

Conversation History:
{history_string}

Concise Summary:`;

    // --- Helper methods to fill placeholders (optional but can improve type safety/readability) ---

    public static fillVerificationCriteriaPrompt(userRequest: string): string {
        return this.VERIFICATION_CRITERIA_PROMPT.replace('{user_request}', userRequest);
    }

    public static fillVerificationPrompt(originalRequest: string, criteria: string, relevantSequence: string): string {
        return this.VERIFICATION_PROMPT
            .replace('{original_request}', originalRequest)
            .replace('{criteria}', criteria)
            .replace('{relevant_history_sequence}', relevantSequence);
    }

     public static fillVerificationFailurePrompt(feedback: string): string {
        return this.VERIFICATION_FAILURE_PROMPT.replace('{feedback}', feedback);
    }

    public static fillCompactionPrompt(historyString: string): string {
        return this.CONVERSATION_COMPACTION_PROMPT.replace('{history_string}', historyString);
    }

    public static fillInvalidToolFormatPrompt(invalidContent: string): string {
        return this.INVALID_TOOL_FORMAT_PROMPT.replace('{invalid_content}', invalidContent);
    }
}
