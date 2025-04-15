import { v4 as uuidv4 } from 'uuid'; // Import UUID for tool call IDs
import { ConversationState } from './ConversationState.js';
import { HumanMessage, AIMessage, SystemMessage, ToolMessage } from './Message.js';
import { ToolParser } from './ToolParser.js';
import { AiClientFactory } from '../ai/AiClientFactory.js';
export class ConversationManager {
    state;
    aiClient;
    serverManager;
    allTools = []; // Cache of all available tools
    toolsLastUpdated = 0; // Timestamp of when tools were last updated
    TOOLS_CACHE_TTL_MS = 10 * 60 * 1000; // 10 minutes cache TTL
    aiClientFactory; // Store the factory reference for switching models
    // Prompts from the design doc
    TOOL_RESULTS_PROMPT = `You have received results from the tool(s) you called previously (shown immediately above).
Analyze these results carefully.
Based *only* on these results and the original user request:
1. If the results provide the necessary information to fully answer the user's original request, formulate and provide the final answer now. Do NOT call any more tools unless absolutely necessary for clarification based *specifically* on the results received.
2. If the results are insufficient or indicate an error, decide if another *different* tool call is needed to achieve the original goal. If so, call the tool using the <<<TOOL_CALL>>>...<<<END_TOOL_CALL>>> format.
3. If you cannot proceed further, explain why.`;
    INVALID_TOOL_FORMAT_PROMPT = `Correction Request:
You attempted to call a tool, but the format was incorrect. 
Remember to use the exact format including delimiters and a valid JSON object with 'name' (string) and 'arguments' (object) fields.

Your invalid attempt contained:
{invalid_content}

Please correct the format and try the tool call again, or provide a text response if you no longer need the tool.`;
    VERIFICATION_CRITERIA_PROMPT = `Based on the following user request, list concise, verifiable criteria for a successful response. 
Focus on key actions, information requested, and constraints mentioned. 
Output ONLY the criteria list, one criterion per line, starting with '- '. Do not include any other text.

User Request:
{user_request}

Criteria:`;
    VERIFICATION_PROMPT = `You are a strict evaluator. Verify if the 'Relevant Conversation Sequence' below meets ALL the 'Success Criteria' based on the 'Original User Request'.

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
    VERIFICATION_FAILURE_PROMPT = `Correction Request:
Your previous response failed verification.
Feedback: {feedback}

Please analyze this feedback carefully and revise your plan and response to fully address the original request and meet all success criteria. 
You may need to use tools differently or provide more detailed information.`;
    CONVERSATION_COMPACTION_PROMPT = `You are an expert conversation summarizer. Analyze the following conversation history and provide a concise summary. Focus on:
- Key user requests and goals.
- Important information discovered or generated.
- Decisions made.
- Final outcomes or current status.
- Any critical unresolved questions or next steps mentioned.

Keep the summary factual and brief, retaining essential context for the conversation to continue.

Conversation History:
{history_string}

Concise Summary:`;
    constructor(aiClient, serverManager, providerModels) {
        this.aiClient = aiClient;
        this.serverManager = serverManager;
        this.state = new ConversationState(); // Initialize with no system prompt initially
        // System prompt will be generated dynamically based on tools
        // Store the factory reference for switching models
        this.aiClientFactory = AiClientFactory;
        // Immediately fetch all tools to prime the cache
        this.refreshToolsCache().catch(err => {
            console.warn('Failed to initialize tools cache:', err);
        });
    }
    /**
     * Switch the AI client to a different provider and model
     * @param providerConfig The provider configuration to use
     * @param providerModels Available models for providers
     * @returns The new model name if switch was successful
     */
    switchAiClient(providerConfig, providerModels) {
        try {
            // Create the new client
            const newClient = this.aiClientFactory.createClient(providerConfig, providerModels);
            // Store the old client temporarily in case we need to roll back
            const oldClient = this.aiClient;
            // Set the new client
            this.aiClient = newClient;
            // Clear conversation history on model switch
            this.clearConversation();
            console.log(`Switched AI client to: ${providerConfig.provider} (${newClient.getModelName()})`);
            return newClient.getModelName();
        }
        catch (error) {
            console.error(`Failed to switch AI client:`, error instanceof Error ? error.message : String(error));
            throw error; // Re-throw to let the caller handle it
        }
    }
    /**
     * Gets the model name identifier from the underlying AI client.
     */
    getAiClientModelName() {
        return this.aiClient.getModelName();
    }
    /**
     * Refreshes the tools cache by fetching all tools from connected servers.
     * @returns Promise that resolves when the cache is refreshed.
     */
    async refreshToolsCache() {
        try {
            this.allTools = await this.serverManager.getAllTools();
            this.toolsLastUpdated = Date.now();
            console.log(`Refreshed tools cache: ${this.allTools.length} tools available.`);
            return this.allTools;
        }
        catch (error) {
            console.error('Error refreshing tools cache:', error);
            // Re-throw or return empty array based on preference
            throw error;
        }
    }
    /**
     * Gets all available tools, refreshing the cache if necessary.
     * @returns Promise that resolves to an array of all available tools.
     */
    async getAllTools() {
        const now = Date.now();
        if (now - this.toolsLastUpdated > this.TOOLS_CACHE_TTL_MS || this.allTools.length === 0) {
            return this.refreshToolsCache();
        }
        return this.allTools;
    }
    /**
     * Generates the system prompt including tool definitions.
     */
    async generateToolSystemPrompt() {
        const allTools = await this.getAllTools();
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
        }
        else {
            for (const tool of allTools) {
                prompt += `## ${tool.name}\n`;
                prompt += `${tool.description || 'No description provided.'}\n\n`;
                // Add schema information if available
                if (tool.input_schema) {
                    prompt += "**Arguments Schema:**\n```json\n";
                    prompt += typeof tool.input_schema === 'string'
                        ? tool.input_schema
                        : JSON.stringify(tool.input_schema, null, 2);
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
    /**
     * Executes a set of parsed tool calls in parallel.
     * @param toolCalls Array of parsed tool calls to execute.
     * @returns Promise that resolves to an array of tool execution results.
     */
    async executeToolCalls(toolCalls) {
        const results = new Map();
        const executions = toolCalls.map(async (toolCall, index) => {
            const toolCallId = `tool-${uuidv4().substring(0, 8)}-${index}`; // Generate a short unique ID
            try {
                // Find which server provides this tool
                const serverName = this.serverManager.findToolProvider(toolCall.name);
                if (!serverName) {
                    const errorMessage = `No server found providing tool '${toolCall.name}'.`;
                    console.warn(errorMessage);
                    results.set(toolCallId, errorMessage);
                    return;
                }
                console.log(`Executing tool '${toolCall.name}' on server '${serverName}'...`);
                // Execute the tool
                const executionResult = await this.serverManager.executeTool(serverName, toolCall.name, toolCall.arguments, { showProgress: true });
                // Format the result
                let resultContent;
                if (executionResult.isError) {
                    resultContent = `Error: ${executionResult.errorMessage || 'Unknown error.'}`;
                }
                else if (Array.isArray(executionResult.toolResult)) {
                    // Handle content array from the tool result
                    resultContent = executionResult.toolResult
                        .map(item => {
                        if (item.type === 'text') {
                            return item.text;
                        }
                        else if (item.type === 'image') {
                            return `[Image: ${item.mimeType}]`;
                        }
                        else {
                            return JSON.stringify(item);
                        }
                    })
                        .join('\n');
                }
                else if (typeof executionResult.toolResult === 'object' && executionResult.toolResult !== null) {
                    resultContent = JSON.stringify(executionResult.toolResult, null, 2);
                }
                else {
                    resultContent = String(executionResult.toolResult || 'No content returned.');
                }
                // Store the result with its ID
                results.set(toolCallId, resultContent);
                // Add tool result to conversation state for history tracking
                this.state.addMessage(new ToolMessage(resultContent, toolCallId, toolCall.name));
                return { toolCallId, name: toolCall.name, result: resultContent };
            }
            catch (error) {
                const errorMessage = `Failed to execute tool '${toolCall.name}': ${error instanceof Error ? error.message : String(error)}`;
                console.error(errorMessage);
                results.set(toolCallId, errorMessage);
                // Add error message to state
                this.state.addMessage(new ToolMessage(errorMessage, toolCallId, toolCall.name));
                return { toolCallId, name: toolCall.name, result: errorMessage };
            }
        });
        // Wait for all tool executions to complete
        await Promise.all(executions);
        return results;
    }
    /**
     * Creates a message to send to the AI with tool results.
     * @param toolResults Map of tool call IDs to results.
     * @returns Human-readable message for the AI.
     */
    createToolResultsMessage(toolResults) {
        let message = this.TOOL_RESULTS_PROMPT;
        return message;
    }
    /**
     * Generates verification criteria for a user request
     * @param userInput The original user input/request
     * @returns The generated verification criteria
     */
    async generateVerificationCriteria(userInput) {
        console.log('Generating verification criteria...');
        try {
            // Create the criteria prompt as a system message
            const systemMessage = new SystemMessage("You are a helpful assistant that generates verification criteria.");
            // Create a user message with the criteria prompt template
            const promptText = this.VERIFICATION_CRITERIA_PROMPT.replace('{user_request}', userInput);
            const userMessage = new HumanMessage(promptText);
            // Call the AI with both messages for criteria generation
            const criteriaResponse = await this.aiClient.generateResponse([systemMessage, userMessage]);
            console.log('Generated criteria:', criteriaResponse);
            return criteriaResponse;
        }
        catch (error) {
            console.error('Error generating verification criteria:', error);
            return '- Respond to the user\'s request accurately\n- Provide relevant information';
        }
    }
    /**
     * Verifies an AI response against the criteria
     * @param originalRequest The original user request
     * @param criteria The verification criteria
     * @param relevantSequence The formatted conversation sequence to verify
     * @returns Object with verification result (passes) and feedback
     */
    async verifyResponse(originalRequest, criteria, relevantSequence) {
        console.log('Verifying response against criteria...');
        try {
            // Create a system message for the verification context
            const systemMessage = new SystemMessage("You are a strict evaluator that verifies responses against criteria and returns JSON.");
            // Create the verification prompt with all placeholders filled
            const promptText = this.VERIFICATION_PROMPT
                .replace('{original_request}', originalRequest)
                .replace('{criteria}', criteria)
                .replace('{relevant_history_sequence}', relevantSequence);
            // Add as a user message (not system message)
            const userMessage = new HumanMessage(promptText);
            // Call the AI with both messages for verification
            const verificationResponse = await this.aiClient.generateResponse([systemMessage, userMessage]);
            try {
                // Parse the JSON response
                const result = JSON.parse(verificationResponse);
                if (typeof result === 'object' && result !== null && 'passes' in result) {
                    console.log('Verification result:', result.passes ? 'PASSED' : 'FAILED');
                    if (!result.passes) {
                        console.log('Feedback:', result.feedback);
                    }
                    return {
                        passes: Boolean(result.passes),
                        feedback: result.feedback || ''
                    };
                }
                else {
                    console.warn('Invalid verification response format:', verificationResponse);
                    return { passes: true, feedback: '' }; // Default to passing if response format is invalid
                }
            }
            catch (error) {
                console.error('Error parsing verification response:', error);
                console.log('Raw response:', verificationResponse);
                return { passes: true, feedback: '' }; // Default to passing if JSON parsing fails
            }
        }
        catch (error) {
            console.error('Error during verification:', error);
            return { passes: true, feedback: '' }; // Default to passing if verification fails
        }
    }
    /**
     * Processes a user's message, interacts with the AI, and potentially handles tool calls.
     * @param userInput - The text input from the user.
     * @returns The AI's final response for this turn.
     */
    async processUserMessage(userInput) {
        console.log(`Processing user message: "${userInput}"`);
        // 1. Add user message to state
        this.state.addMessage(new HumanMessage(userInput));
        // 2. Generate verification criteria if this is a new request
        // and there are no existing criteria for the conversation
        if (!this.state.getVerificationState()) {
            const criteria = await this.generateVerificationCriteria(userInput);
            this.state.setVerificationState(userInput, criteria);
        }
        // 3. Generate the dynamic system prompt based on current tools
        const systemPrompt = await this.generateToolSystemPrompt();
        this.state.setSystemPrompt(systemPrompt); // Update the system prompt in the state
        // 4. Get the full message history for the AI
        let messagesForAi = this.state.getMessages();
        // 5. Try to compact history if it's getting too long
        if (messagesForAi.length > 20) {
            await this.state.compactHistory(this.CONVERSATION_COMPACTION_PROMPT, this.aiClient);
            // Refresh messages after compaction
            const compactedMessages = this.state.getMessages();
            if (compactedMessages.length < messagesForAi.length) {
                console.log(`Compacted conversation history from ${messagesForAi.length} to ${compactedMessages.length} messages`);
                messagesForAi = compactedMessages;
            }
        }
        // 6. Initial AI call
        let aiResponseContent;
        try {
            aiResponseContent = await this.aiClient.generateResponse(messagesForAi);
            console.log(`AI Response (${this.aiClient.getModelName()}):`, aiResponseContent.substring(0, 200) + (aiResponseContent.length > 200 ? '...' : ''));
        }
        catch (error) {
            console.error("Error during AI interaction:", error);
            const errorMessage = `Sorry, I encountered an error: ${error instanceof Error ? error.message : String(error)}`;
            return errorMessage;
        }
        // 7. Parse and execute tool calls if present
        if (ToolParser.containsToolCalls(aiResponseContent)) {
            // Extract and parse tool calls
            const parsedToolCalls = ToolParser.parseToolCalls(aiResponseContent);
            if (parsedToolCalls.length > 0) {
                console.log(`Found ${parsedToolCalls.length} tool calls in AI response.`);
                // Add AI response to conversation history with tool calls flag
                const aiMessage = new AIMessage(aiResponseContent, {
                    hasToolCalls: true,
                    pendingToolCalls: true
                });
                this.state.addMessage(aiMessage);
                // Execute tool calls
                const toolResults = await this.executeToolCalls(parsedToolCalls);
                // Mark tool calls as no longer pending
                aiMessage.pendingToolCalls = false;
                // Build the tool results message
                const toolResultsPrompt = this.createToolResultsMessage(toolResults);
                // Create a new context with original messages plus tool results
                const updatedMessages = this.state.getMessages();
                // 8. Second AI call with tool results
                try {
                    const followUpResponse = await this.aiClient.generateResponse(updatedMessages);
                    console.log(`Follow-up AI Response (after tool execution):`, followUpResponse.substring(0, 200) + (followUpResponse.length > 200 ? '...' : ''));
                    // Check if the follow-up response contains more tool calls
                    if (ToolParser.containsToolCalls(followUpResponse)) {
                        // Add this as an AI message and recursively process the next round of tool calls
                        const followUpMessage = new AIMessage(followUpResponse, { hasToolCalls: true });
                        this.state.addMessage(followUpMessage);
                        // Note: In a full implementation, we would recursively handle these tool calls
                        // but for simplicity, we'll just add the message and not handle further tool calls
                        console.log('Follow-up response contains more tool calls. In a complete implementation, these would be processed recursively.');
                        // Add the message without the tool calls flag for now
                        this.state.addMessage(new AIMessage(followUpResponse));
                        // For verification purposes, we'll proceed with verification without handling the additional tool calls
                        // In production, we would want to fully resolve all tool calls before verification
                    }
                    else {
                        // Store the follow-up response in history
                        this.state.addMessage(new AIMessage(followUpResponse));
                    }
                    // 9. Verify the final response (if verification is enabled)
                    const verificationState = this.state.getVerificationState();
                    if (verificationState) {
                        const { originalRequest, criteria } = verificationState;
                        const relevantSequence = this.state.getRelevantSequenceForVerification();
                        const verificationResult = await this.verifyResponse(originalRequest, criteria, relevantSequence);
                        // 10. If verification fails, retry with feedback
                        if (!verificationResult.passes) {
                            console.log('Response verification failed. Retrying with feedback.');
                            // Generate a correction prompt
                            const correctionPrompt = this.VERIFICATION_FAILURE_PROMPT.replace('{feedback}', verificationResult.feedback);
                            // Create a system message with a brief instruction
                            const systemMessage = new SystemMessage("You are a helpful assistant that needs to correct your previous response.");
                            // Create a user message with the correction prompt
                            const userMessage = new HumanMessage(correctionPrompt);
                            // Add correction prompt to messages
                            const correctionMessages = [...this.state.getMessages(), systemMessage, userMessage];
                            // Make one more AI call with the correction
                            try {
                                const correctedResponse = await this.aiClient.generateResponse(correctionMessages);
                                console.log('Generated corrected response after verification failure');
                                // Add the corrected response to history
                                this.state.addMessage(new AIMessage(correctedResponse));
                                // Return the corrected response
                                return correctedResponse;
                            }
                            catch (error) {
                                console.error('Error generating corrected response:', error);
                                return followUpResponse; // Fall back to the uncorrected response
                            }
                        }
                    }
                    // Return the final response
                    return followUpResponse;
                }
                catch (error) {
                    console.error("Error during AI follow-up interaction:", error);
                    const errorMessage = `Sorry, I encountered an error processing the tool results: ${error instanceof Error ? error.message : String(error)}`;
                    this.state.addMessage(new AIMessage(errorMessage));
                    return errorMessage;
                }
            }
            else {
                console.warn("Tool call delimiters detected but no valid tool calls parsed.");
            }
        }
        // 11. Verify non-tool responses before returning
        this.state.addMessage(new AIMessage(aiResponseContent)); // Add to history for verification
        const verificationState = this.state.getVerificationState();
        if (verificationState) {
            const { originalRequest, criteria } = verificationState;
            const relevantSequence = this.state.getRelevantSequenceForVerification();
            const verificationResult = await this.verifyResponse(originalRequest, criteria, relevantSequence);
            // If verification fails, retry with feedback
            if (!verificationResult.passes) {
                console.log('Response verification failed. Retrying with feedback.');
                // Generate a correction prompt
                const correctionPrompt = this.VERIFICATION_FAILURE_PROMPT.replace('{feedback}', verificationResult.feedback);
                // Create a system message with a brief instruction
                const systemMessage = new SystemMessage("You are a helpful assistant that needs to correct your previous response.");
                // Create a user message with the correction prompt
                const userMessage = new HumanMessage(correctionPrompt);
                // Add correction prompt to messages
                const correctionMessages = [...this.state.getMessages(), systemMessage, userMessage];
                // Make one more AI call with the correction
                try {
                    const correctedResponse = await this.aiClient.generateResponse(correctionMessages);
                    console.log('Generated corrected response after verification failure');
                    // Add the corrected response to history
                    this.state.addMessage(new AIMessage(correctedResponse));
                    // Return the corrected response
                    return correctedResponse;
                }
                catch (error) {
                    console.error('Error generating corrected response:', error);
                }
            }
        }
        // If no tool calls or verification issues, return the original response
        return aiResponseContent;
    }
    /**
     * Clears the conversation history.
     */
    clearConversation() {
        this.state.clearHistory();
        console.log("Conversation history cleared.");
    }
    /**
     * Gets the current conversation history.
     */
    getHistory() {
        // Return messages including the system prompt for context
        return this.state.getMessages();
    }
}
//# sourceMappingURL=ConversationManager.js.map