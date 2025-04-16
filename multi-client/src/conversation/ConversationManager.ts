import { v4 as uuidv4 } from 'uuid'; // Import UUID for tool call IDs
import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import type { IAiClient } from '../ai/IAiClient.js';
import type { ServerManager } from '../ServerManager.js';
import { ConversationState } from './ConversationState.js';
import { HumanMessage, AIMessage, SystemMessage, ToolMessage } from './Message.js';
import type { ConversationMessage } from './Message.js';
import type { Tool } from '@modelcontextprotocol/sdk/types.js'; // MCP Tool type
import { ToolParser, type ParsedToolCall } from './ToolParser.js';
import { AiClientFactory } from '../ai/AiClientFactory.js';
import type { AiProviderConfig, ProviderModelsStructure } from '../types.js';

// Use ES module approach for __dirname equivalent
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const baseDir = path.join(__dirname, '../../..');

// Interface for serialized conversations
interface SerializedConversation {
  id: string;
  title: string;
  modelName: string;
  provider: string;
  createdAt: string;
  updatedAt: string;
  messages: {
    role: string;
    content: string;
    hasToolCalls?: boolean;
    pendingToolCalls?: boolean;
  }[];
}

export class ConversationManager {
  private state: ConversationState;
  private aiClient: IAiClient;
  private serverManager: ServerManager;
  private allTools: Tool[] = []; // Cache of all available tools
  private toolsLastUpdated: number = 0; // Timestamp of when tools were last updated
  private readonly TOOLS_CACHE_TTL_MS = 10 * 60 * 1000; // 10 minutes cache TTL
  private aiClientFactory: typeof AiClientFactory; // Store the factory reference for switching models
  
  // Conversation persistence properties
  private conversationsDir: string;
  private currentConversationId: string;
  private saveDebounceTimeout: NodeJS.Timeout | null = null;
  
  // Prompts from the design doc
  private readonly TOOL_RESULTS_PROMPT = `You have received results from the tool(s) you called previously (shown immediately above).
Analyze these results carefully.
Based *only* on these results and the original user request:
1. If the results provide the necessary information to fully answer the user's original request, formulate and provide the final answer now. Do NOT call any more tools unless absolutely necessary for clarification based *specifically* on the results received.
2. If the results are insufficient or indicate an error, decide if another *different* tool call is needed to achieve the original goal. If so, call the tool using the <<<TOOL_CALL>>>...<<<END_TOOL_CALL>>> format.
3. If you cannot proceed further, explain why.`;

  private readonly INVALID_TOOL_FORMAT_PROMPT = `Correction Request:
You attempted to call a tool, but the format was incorrect. 
Remember to use the exact format including delimiters and a valid JSON object with 'name' (string) and 'arguments' (object) fields.

Your invalid attempt contained:
{invalid_content}

Please correct the format and try the tool call again, or provide a text response if you no longer need the tool.`;

  private readonly VERIFICATION_CRITERIA_PROMPT = `Based on the following user request, list concise, verifiable criteria for a successful response. 
Focus on key actions, information requested, and constraints mentioned. 
Output ONLY the criteria list, one criterion per line, starting with '- '. Do not include any other text.

User Request:
{user_request}

Criteria:`;

  private readonly VERIFICATION_PROMPT = `You are a strict evaluator. Verify if the 'Relevant Conversation Sequence' below meets ALL the 'Success Criteria' based on the 'Original User Request'.

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

  private readonly VERIFICATION_FAILURE_PROMPT = `Your previous response failed verification based on the following feedback:
{feedback}

Revise your response to fully address the original request and meet all success criteria based on this feedback. Provide only the corrected response.`;

  private readonly CONVERSATION_COMPACTION_PROMPT = `You are an expert conversation summarizer. Analyze the following conversation history and provide a concise summary. Focus on:
- Key user requests and goals.
- Important information discovered or generated.
- Decisions made.
- Final outcomes or current status.
- Any critical unresolved questions or next steps mentioned.

Keep the summary factual and brief, retaining essential context for the conversation to continue.

Conversation History:
{history_string}

Concise Summary:`;

  constructor(
    aiClient: IAiClient, 
    serverManager: ServerManager, 
    providerModels: ProviderModelsStructure
  ) {
    this.aiClient = aiClient;
    this.serverManager = serverManager;
    this.state = new ConversationState(); // Initialize with no system prompt initially
    // System prompt will be generated dynamically based on tools
    
    // Store the factory reference for switching models
    this.aiClientFactory = AiClientFactory;
    
    // Set up conversations directory
    this.conversationsDir = path.join(baseDir, 'conversations');
    this.ensureConversationsDir();
    
    // Generate a new conversation ID for this session
    this.currentConversationId = uuidv4();
    
    // Immediately fetch all tools to prime the cache
    this.refreshToolsCache().catch(err => {
      console.warn('Failed to initialize tools cache:', err);
    });
  }
  
  /**
   * Ensures the conversations directory exists
   */
  private ensureConversationsDir(): void {
    try {
      if (!fs.existsSync(this.conversationsDir)) {
        fs.mkdirSync(this.conversationsDir, { recursive: true });
        console.log(`Created conversations directory at: ${this.conversationsDir}`);
      }
    } catch (error) {
      console.error(`Error creating conversations directory:`, error);
    }
  }
  
  /**
   * Saves the current conversation to disk
   */
  private saveConversation(): void {
    if (this.saveDebounceTimeout) {
      clearTimeout(this.saveDebounceTimeout);
    }
    
    // Debounce save operations to prevent excessive disk writes
    this.saveDebounceTimeout = setTimeout(() => {
      try {
        const messages = this.state.getMessages();
        
        // Don't save if there are no messages
        if (messages.length === 0) {
          return;
        }
        
        // Generate a title from the first few user messages
        let title = 'New Conversation';
        const userMessages = messages.filter(m => m._getType() === 'human');
        
        if (userMessages.length > 0) {
          // Use the first user message as the title, limited to 50 chars
          const firstMessage = userMessages[0].content.toString();
          title = firstMessage.length > 50 
            ? firstMessage.substring(0, 47) + '...' 
            : firstMessage;
        }
        
        // Create serialized conversation
        const conversation: SerializedConversation = {
          id: this.currentConversationId,
          title,
          modelName: this.getAiClientModelName(),
          provider: this.aiClient.getProvider?.() || 'unknown',
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
          messages: messages.map(msg => ({
            role: msg._getType(),
            content: typeof msg.content === 'string' ? msg.content : JSON.stringify(msg.content),
            hasToolCalls: (msg as any).hasToolCalls,
            pendingToolCalls: (msg as any).pendingToolCalls
          }))
        };
        
        // Write to file
        const filePath = path.join(this.conversationsDir, `${this.currentConversationId}.json`);
        fs.writeFileSync(filePath, JSON.stringify(conversation, null, 2), 'utf-8');
        console.log(`Saved conversation to: ${filePath}`);
      } catch (error) {
        console.error('Error saving conversation:', error);
      }
    }, 1000); // 1 second debounce
  }
  
  /**
   * Loads a conversation from disk
   * @param conversationId The ID of the conversation to load
   * @returns true if successful, false otherwise
   */
  public loadConversation(conversationId: string): boolean {
    try {
      const filePath = path.join(this.conversationsDir, `${conversationId}.json`);
      
      if (!fs.existsSync(filePath)) {
        console.error(`Conversation file not found: ${filePath}`);
        return false;
      }
      
      const conversationData = fs.readFileSync(filePath, 'utf-8');
      const conversation: SerializedConversation = JSON.parse(conversationData);
      
      // Clear current conversation
      this.state.clearHistory();
      
      // Set current conversation ID
      this.currentConversationId = conversation.id;
      
      // Reconstruct messages
      for (const msg of conversation.messages) {
        if (msg.role === 'system') {
          this.state.addMessage(new SystemMessage(msg.content));
        } else if (msg.role === 'human') {
          this.state.addMessage(new HumanMessage(msg.content));
        } else if (msg.role === 'ai') {
          this.state.addMessage(new AIMessage(msg.content, {
            hasToolCalls: msg.hasToolCalls,
            pendingToolCalls: msg.pendingToolCalls
          }));
        } else if (msg.role === 'tool') {
          // Tool messages need a proper tool call ID and name which we don't have
          // For now, we'll create them with placeholder values
          this.state.addMessage(new ToolMessage(
            msg.content,
            'restored-tool-call-id',
            'restored-tool'
          ));
        }
      }
      
      // Emit events or return the loaded state
      console.log(`Loaded conversation: ${conversation.title}`);
      return true;
    } catch (error) {
      console.error('Error loading conversation:', error);
      return false;
    }
  }
  
  /**
   * Lists all saved conversations
   * @returns Array of conversation metadata
   */
  public listConversations(): any[] {
    try {
      // Ensure conversations directory exists
      this.ensureConversationsDir();
      
      // Read conversation files
      const files = fs.readdirSync(this.conversationsDir)
        .filter(file => file.endsWith('.json'));
        
      // Extract metadata
      const conversations = files.map(file => {
        try {
          const filePath = path.join(this.conversationsDir, file);
          const data = fs.readFileSync(filePath, 'utf-8');
          const conversation: SerializedConversation = JSON.parse(data);
          
          return {
            id: conversation.id,
            title: conversation.title,
            modelName: conversation.modelName,
            provider: conversation.provider,
            createdAt: conversation.createdAt,
            updatedAt: conversation.updatedAt,
            messageCount: conversation.messages.length,
            isActive: conversation.id === this.currentConversationId
          };
        } catch (error) {
          console.warn(`Error parsing conversation file: ${file}`, error);
          return null;
        }
      }).filter(Boolean); // Remove nulls
      
      // Sort by updatedAt, most recent first
      return conversations.sort((a, b) => {
        if (!a || !b) return 0;
        return new Date(b.updatedAt || '').getTime() - new Date(a.updatedAt || '').getTime();
      });
    } catch (error) {
      console.error('Error listing conversations:', error);
      return [];
    }
  }
  
  /**
   * Creates a new empty conversation
   */
  public newConversation(): void {
    // Clear current state
    this.state.clearHistory();
    
    // Generate new ID
    this.currentConversationId = uuidv4();
    
    console.log(`Created new conversation with ID: ${this.currentConversationId}`);
  }
  
  /**
   * Switch the AI client to a different provider and model
   * @param providerConfig The provider configuration to use
   * @param providerModels Available models for providers
   * @returns The new model name if switch was successful
   */
  public async switchAiClient( // Mark function as async
    providerConfig: AiProviderConfig,
    providerModels: ProviderModelsStructure
  ): Promise<string> { // Return a Promise<string>
    try {
      // Fetch tools before creating the client
      // Note: This assumes getAllTools() is efficient or cached.
      // Consider passing tools if already available.
      const tools = await this.getAllTools();

      // Create the new client, passing available tools
      const newClient = this.aiClientFactory.createClient(providerConfig, providerModels, tools);

      // Store the old client temporarily in case we need to roll back
      const oldClient = this.aiClient;
      
      // Set the new client
      this.aiClient = newClient;
      
      // Clear conversation history on model switch
      this.clearConversation();
      
      console.log(`Switched AI client to: ${providerConfig.provider} (${newClient.getModelName()})`);
      return newClient.getModelName();
    } catch (error) {
      console.error(`Failed to switch AI client:`, error instanceof Error ? error.message : String(error));
      throw error; // Re-throw to let the caller handle it
    }
  }

  /**
   * Gets the model name identifier from the underlying AI client.
   */
  public getAiClientModelName(): string {
    return this.aiClient.getModelName();
  }

  /**
   * Gets the provider name identifier from the underlying AI client.
   */
  public getAiProviderName(): string {
    return this.aiClient.getProvider?.() || 'unknown';
  }

  /**
   * Refreshes the tools cache by fetching all tools from connected servers.
   * @returns Promise that resolves when the cache is refreshed.
   */
  private async refreshToolsCache(): Promise<Tool[]> {
    try {
      this.allTools = await this.serverManager.getAllTools();
      this.toolsLastUpdated = Date.now();
      console.log(`Refreshed tools cache: ${this.allTools.length} tools available.`);
      return this.allTools;
    } catch (error) {
      console.error('Error refreshing tools cache:', error);
      // Re-throw or return empty array based on preference
      throw error;
    }
  }

  /**
   * Gets all available tools, refreshing the cache if necessary.
   * @returns Promise that resolves to an array of all available tools.
   */
  private async getAllTools(): Promise<Tool[]> {
    const now = Date.now();
    if (now - this.toolsLastUpdated > this.TOOLS_CACHE_TTL_MS || this.allTools.length === 0) {
      return this.refreshToolsCache();
    }
    return this.allTools;
  }

  /**
   * Generates the system prompt including tool definitions.
   */
  private async generateToolSystemPrompt(): Promise<string> {
    const allTools: Tool[] = await this.getAllTools();

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
   * Executes a set of parsed tool calls (now including generated IDs) in parallel.
   * Executes a set of tool calls.
   * @param toolCalls Array of tool calls, each including an `id`, `name`, and `args`.
   * @returns Promise that resolves to a map of tool call IDs to their string results.
   */
  private async executeToolCalls(
    // Standardize on 'args' property name, matching LangChain
    toolCalls: { id: string; name: string; args: Record<string, any> }[]
  ): Promise<Map<string, string>> {
    const results = new Map<string, string>();
    const executions = toolCalls.map(async (toolCall) => {
      const toolCallId = toolCall.id;
      const toolName = toolCall.name;
      const toolArgs = toolCall.args; // Use 'args'

      try {
        const serverName = this.serverManager.findToolProvider(toolName);
        if (!serverName) {
          const errorMessage = `No server found providing tool '${toolName}'.`;
          console.warn(errorMessage);
          results.set(toolCallId, errorMessage);
          return;
        }

        console.log(`Executing tool '${toolName}' on server '${serverName}'...`);

        // Execute the tool using name and args from toolCall
        const executionResult = await this.serverManager.executeTool(
          serverName,
          toolName,
          toolArgs,
          { showProgress: true }
        );

        // Format the result
        let resultContent: string;
        if (executionResult.isError) {
          resultContent = `Error: ${executionResult.errorMessage || 'Unknown error.'}`;
        } else if (Array.isArray(executionResult.toolResult)) {
          // Handle content array from the tool result
          resultContent = executionResult.toolResult
            .map(item => {
              if (item.type === 'text') {
                return item.text;
              } else if (item.type === 'image') {
                return `[Image: ${item.mimeType}]`;
              } else {
                return JSON.stringify(item);
              }
            })
            .join('\n');
        } else if (typeof executionResult.toolResult === 'object' && executionResult.toolResult !== null) {
          resultContent = JSON.stringify(executionResult.toolResult, null, 2);
        } else {
          resultContent = String(executionResult.toolResult || 'No content returned.');
        }

        // Store the result with its ID
        results.set(toolCallId, resultContent);

        // DO NOT add ToolMessage to state here. Return the result instead.
        // this.state.addMessage(new ToolMessage(
        //   resultContent,
        //   toolCallId,
        //   toolCall.name
        // ));

        // Return enough info to create the ToolMessage later
        return { toolCallId, name: toolCall.name, result: resultContent }; 
      } catch (error) {
        const errorMessage = `Failed to execute tool '${toolCall.name}': ${error instanceof Error ? error.message : String(error)}`;
        console.error(errorMessage);
        results.set(toolCallId, errorMessage);

        // DO NOT add ToolMessage to state here. Return the error result instead.
        // this.state.addMessage(new ToolMessage(
        //   errorMessage,
        //   toolCallId,
        //   toolName // Use toolName from the loop
        // ));

        // Return enough info to create the ToolMessage later
        return { toolCallId, name: toolName, result: errorMessage };
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
  private createToolResultsMessage(toolResults: Map<string, string>): string {
    let message = this.TOOL_RESULTS_PROMPT;
    return message;
  }

  /**
   * Generates verification criteria for a user request
   * @param userInput The original user input/request
   * @returns The generated verification criteria
   */
  private async generateVerificationCriteria(userInput: string): Promise<string> {
    console.log('Generating verification criteria...');
    try {
      // Create the criteria prompt
      const promptText = this.VERIFICATION_CRITERIA_PROMPT.replace('{user_request}', userInput);
      
      // Create a temporary array with system message first, then user message
      const messages = [
        new SystemMessage("You are a helpful assistant that generates verification criteria."),
        new HumanMessage(promptText)
      ];
      
      // Call the AI with the properly ordered messages for criteria generation
      const criteriaResponse = await this.aiClient.generateResponse(messages);
      console.log('Generated criteria:', criteriaResponse);
      
      return criteriaResponse;
    } catch (error) {
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
  private async verifyResponse(
    originalRequest: string, 
    criteria: string, 
    relevantSequence: string
  ): Promise<{ passes: boolean, feedback: string }> {
    console.log('Verifying response against criteria...');
    
    try {
      // Create the verification prompt with all placeholders filled
      const promptText = this.VERIFICATION_PROMPT
        .replace('{original_request}', originalRequest)
        .replace('{criteria}', criteria)
        .replace('{relevant_history_sequence}', relevantSequence);
      
      // Create messages array with system message first
      const messages = [
        new SystemMessage("You are a strict evaluator that verifies responses against criteria and returns JSON."),
        new HumanMessage(promptText)
      ];
      
      // Call the AI with properly ordered messages for verification
      const verificationResponse = await this.aiClient.generateResponse(messages);
      
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
        } else {
          console.warn('Invalid verification response format:', verificationResponse);
          return { passes: true, feedback: '' }; // Default to passing if response format is invalid
        }
      } catch (error) {
        console.error('Error parsing verification response:', error);
        console.log('Raw response:', verificationResponse);
        return { passes: true, feedback: '' }; // Default to passing if JSON parsing fails
      }
    } catch (error) {
      console.error('Error during verification:', error);
      return { passes: true, feedback: '' }; // Default to passing if verification fails
    }
  }
  
  /**
   * Processes a user's message, interacts with the AI, and potentially handles tool calls.
   * @param userInput - The text input from the user.
   * @returns The AI's final response for this turn.
   */
  async processUserMessage(userInput: string): Promise<string> {
    console.log(`Processing user message: "${userInput}"`);

    // 1. Add user message to state
    this.state.addMessage(new HumanMessage(userInput));
    
    // Save conversation after adding user message
    this.saveConversation();

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
    let aiResponseContent: string;
    try {
      aiResponseContent = await this.aiClient.generateResponse(messagesForAi);
      console.log(`AI Response (${this.aiClient.getModelName()}):`, aiResponseContent.substring(0, 200) + (aiResponseContent.length > 200 ? '...' : ''));
    } catch (error) {
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
        
        // Add IDs to tool calls before executing
        const toolCallsWithIds = parsedToolCalls.map(call => ({
          ...call,
          id: `call-${Date.now()}-${Math.floor(Math.random() * 1000)}` // Generate ID for parser-based calls
        }));

        // Map `arguments` from parser to `args` for executeToolCalls
        const toolCallsForExecution = toolCallsWithIds.map(tc => ({
            id: tc.id,
            name: tc.name,
            args: tc.arguments // Map 'arguments' to 'args'
        }));

        // Execute tool calls
        const toolResults = await this.executeToolCalls(toolCallsForExecution);

        // Mark tool calls as no longer pending
        aiMessage.pendingToolCalls = false;
        
        // Build the tool results message
        const toolResultsPrompt = this.createToolResultsMessage(toolResults);
        
        // Create a new context with original messages plus tool results
        const updatedMessages = this.state.getMessages();
        
        // 8. Second AI call with tool results
        try {
          const followUpResponse = await this.aiClient.generateResponse(updatedMessages);
          console.log(`Follow-up AI Response (after tool execution):`, 
            followUpResponse.substring(0, 200) + (followUpResponse.length > 200 ? '...' : ''));
          
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
          } else {
            // Store the follow-up response in history
            this.state.addMessage(new AIMessage(followUpResponse));
          }
          
          // 9. Verify the final response (if verification is enabled)
          const verificationState = this.state.getVerificationState();
          if (verificationState) {
            const { originalRequest, criteria } = verificationState;
            const relevantSequence = this.state.getRelevantSequenceForVerification();
            
            const verificationResult = await this.verifyResponse(
              originalRequest, 
              criteria, 
              relevantSequence
            );
            
            // 10. If verification fails, retry with feedback
            if (!verificationResult.passes) {
              console.log('Response verification failed. Retrying with feedback.');
              
              // Generate a correction prompt
              const correctionPrompt = this.VERIFICATION_FAILURE_PROMPT.replace(
                '{feedback}', 
                verificationResult.feedback
              );

              // Create a user message containing the correction prompt
              const correctionUserMessage = new HumanMessage(correctionPrompt);

              // Get the current history (which includes the original system prompt at the start)
              // and add the correction request as the latest human message.
              const correctionMessages = [
                  ...this.state.getMessages(),
                  correctionUserMessage
              ];

              // Make one more AI call with the correction context
              try {
                const correctedResponse = await this.aiClient.generateResponse(correctionMessages);
                console.log('Generated corrected response after verification failure');
                
                // Add the corrected response to history
                this.state.addMessage(new AIMessage(correctedResponse));
                
                // Return the corrected response
                return correctedResponse;
              } catch (error) {
                console.error('Error generating corrected response:', error);
                return followUpResponse; // Fall back to the uncorrected response
              }
            }
          }
          
          // Return the final response
          return followUpResponse;
        } catch (error) {
          console.error("Error during AI follow-up interaction:", error);
          const errorMessage = `Sorry, I encountered an error processing the tool results: ${error instanceof Error ? error.message : String(error)}`;
          this.state.addMessage(new AIMessage(errorMessage));
          return errorMessage;
        }
      } else {
        console.warn("Tool call delimiters detected but no valid tool calls parsed.");
      }
    }
    
    // Initialize variables for the tool call loop
    let currentResponse = aiResponseContent; // Start with the initial AI response
    let toolRound = 0;
    const maxToolRounds = 5; // Limit recursive tool calls
    
    // We will determine the single final AI message later and add it once.

    // Loop while the response contains tool calls and we haven't hit the limit
    while (ToolParser.containsToolCalls(currentResponse) && toolRound < maxToolRounds) {
      toolRound++;
      console.log(`--- Tool Call Round ${toolRound} ---`);

      // Add the AI response that *requested* the tools.
      // LangChain's AIMessage should automatically parse tool calls from providers like Anthropic
      // into the standard `tool_calls` property.
      const aiMessageRequestingTools = new AIMessage(currentResponse, {
          // Let LangChain handle populating tool_calls if the underlying model supports it
      });

      // Check if the AIMessage actually contains tool calls (using the standard property)
      // Filter out any tool calls that might have an undefined ID (important for robustness)
      const actualToolCalls = (aiMessageRequestingTools.tool_calls || []).filter(
          (tc): tc is { id: string; name: string; args: Record<string, any> } => tc.id !== undefined
      );


      if (actualToolCalls.length === 0) {
          console.log("AI response did not contain valid standard tool calls (with IDs). Exiting tool loop.");
          // It might contain the <<<TOOL_CALL>>> delimiters but wasn't parsed correctly by LangChain,
          // or the AI decided not to call tools this round, or the calls lacked IDs.
          // or the AI decided not to call tools this round.
          break; // Exit the loop
      }

      console.log(`Found ${actualToolCalls.length} standard tool calls in AI response.`);

      // Add the message requesting tools to history *before* execution
      // Mark as pending
      aiMessageRequestingTools.hasToolCalls = true;
      aiMessageRequestingTools.pendingToolCalls = true;
      this.state.addMessage(aiMessageRequestingTools);

      // Execute tool calls using the IDs provided by the AI/LangChain
      const toolResultsMap = await this.executeToolCalls(actualToolCalls);

      // Mark the tool calls in the previous AI message as no longer pending
      aiMessageRequestingTools.pendingToolCalls = false;

      // Add the tool results back using ToolMessage, linked by the correct ID
      // Use the filtered actualToolCalls which guarantees IDs are strings
      for (const toolCall of actualToolCalls) {
          // toolCall.id is guaranteed to be a string here due to the filter above
          const result = toolResultsMap.get(toolCall.id) || `Error: Result not found for tool call ${toolCall.id}`;
          this.state.addMessage(new ToolMessage(
              result,
              toolCall.id,
              toolCall.name
          ));
      }
      
      // Get the updated message history including the AIMessage requesting tools and the ToolMessages containing results
      const messagesForFollowUp = this.state.getMessages();
      
      // Make the next AI call with the tool results context
      try {
        currentResponse = await this.aiClient.generateResponse(messagesForFollowUp);
        console.log(`AI Response (Round ${toolRound + 1}):`, 
          currentResponse.substring(0, 200) + (currentResponse.length > 200 ? '...' : ''));
      } catch (error) {
        console.error(`Error during AI follow-up interaction (Round ${toolRound + 1}):`, error);
        const errorMessage = `Sorry, I encountered an error processing the tool results: ${error instanceof Error ? error.message : String(error)}`;
        this.state.addMessage(new AIMessage(errorMessage)); // Add error message to history
        currentResponse = errorMessage; // Set response to error message
        break; // Exit loop on error
      }
    } // End while loop

    if (toolRound >= maxToolRounds) {
        console.warn(`Reached maximum tool call rounds (${maxToolRounds}). Proceeding with last response.`);
        // The last response might still contain tool calls, which will be ignored now.
    }
    
    // We will add the final AI message *after* potential verification corrections.
    let finalAiResponseContent = currentResponse; // Store the content string

    // 9. Verify the final response (if verification is enabled)
    const verificationState = this.state.getVerificationState();
    if (verificationState) {
      const { originalRequest, criteria } = verificationState;
      const relevantSequence = this.state.getRelevantSequenceForVerification();
      
      const verificationResult = await this.verifyResponse(
        originalRequest, 
        criteria, 
        relevantSequence
      );
      
      // Attach verification result to the last AI message for potential UI display
      const lastMessageIndex = this.state.getHistoryWithoutSystemPrompt().length - 1;
      if (lastMessageIndex >= 0) {
          const lastMessage = this.state.getHistoryWithoutSystemPrompt()[lastMessageIndex];
          if (lastMessage instanceof AIMessage) {
              (lastMessage as any).verificationResult = verificationResult; // Add verification result
          }
      }

      // 10. If verification fails, retry with feedback
      if (!verificationResult.passes) {
        console.log('Response verification failed. Retrying with feedback.');
        
        // Generate a correction prompt
        const correctionPrompt = this.VERIFICATION_FAILURE_PROMPT.replace(
          '{feedback}', 
          verificationResult.feedback
        );
        
        // Create messages for the correction call
        // Use the *full* message history (which includes the original system prompt)
        // and append the correction request as the latest human message.
        const correctionMessages = [
            ...this.state.getMessages(), // Get all messages including the original system prompt
            new HumanMessage(correctionPrompt) // Add the correction request as the last message
        ];
        
        // Make one more AI call with the correction
        try {
          const correctedResponse = await this.aiClient.generateResponse(correctionMessages);
          console.log('Generated corrected response after verification failure');

          // Update the content to be returned and added later
          finalAiResponseContent = correctedResponse;

          // Add the corrected response to history *immediately* so it's part of the state
          // before the final addMessage check later (this replaces the failed one implicitly)
          this.state.addMessage(new AIMessage(correctedResponse));

          // We will save and return finalAiResponseContent later
        } catch (error) {
          console.error('Error generating corrected response:', error);
          // Fall back to the uncorrected response content if correction fails
          // finalAiResponseContent remains the uncorrected 'currentResponse'
          this.saveConversation(); // Save even if correction failed
        }
      }
    }
    
    // Add the single, definitive final AI message for this turn to the state
    // Use the potentially corrected content from verification
    const finalAiMessage = new AIMessage(finalAiResponseContent, { hasToolCalls: false });
    // Check if the last message added was already this exact response (e.g., from failed verification retry)
    const history = this.state.getHistoryWithoutSystemPrompt();
    const lastMessage = history[history.length - 1];
    if (!(lastMessage instanceof AIMessage && lastMessage.content === finalAiMessage.content)) {
         this.state.addMessage(finalAiMessage);
    } else {
        console.log("Skipping adding duplicate final AI message after verification handling.");
    }


    // Save the conversation after adding the final AI response
    this.saveConversation();
    
    // Return the final response content string
    return finalAiResponseContent;
  }

  /**
   * Clears the conversation history.
   */
  clearConversation(): void {
    this.state.clearHistory();
    
    // Generate a new conversation ID for the cleared conversation
    this.currentConversationId = uuidv4();
    
    console.log("Conversation history cleared. New conversation ID: " + this.currentConversationId);
  }

  /**
   * Gets the current conversation history.
   */
  getHistory(): ConversationMessage[] {
    // Return messages including the system prompt for context
    return this.state.getMessages();
  }
  
  /**
   * Gets the current conversation metadata
   */
  getCurrentConversation(): any {
    try {
      const filePath = path.join(this.conversationsDir, `${this.currentConversationId}.json`);
      
      if (fs.existsSync(filePath)) {
        const conversationData = fs.readFileSync(filePath, 'utf-8');
        return JSON.parse(conversationData);
      } else {
        // If file doesn't exist yet, return basic metadata
        return {
          id: this.currentConversationId,
          title: 'New Conversation',
          modelName: this.getAiClientModelName(),
          provider: this.aiClient.getProvider?.() || 'unknown',
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
          messageCount: this.state.getMessages().length,
        };
      }
    } catch (error) {
      console.error('Error getting current conversation:', error);
      return {
        id: this.currentConversationId,
        title: 'New Conversation',
        error: 'Failed to load conversation data'
      };
    }
  }
  
  /**
   * Renames a conversation
   * @param conversationId The ID of the conversation to rename
   * @param newTitle The new title for the conversation
   * @returns true if successful, false otherwise
   */
  public renameConversation(conversationId: string, newTitle: string): boolean {
    try {
      const filePath = path.join(this.conversationsDir, `${conversationId}.json`);
      
      if (!fs.existsSync(filePath)) {
        console.error(`Conversation file not found: ${filePath}`);
        return false;
      }
      
      const conversationData = fs.readFileSync(filePath, 'utf-8');
      const conversation: SerializedConversation = JSON.parse(conversationData);
      
      // Update title
      conversation.title = newTitle;
      conversation.updatedAt = new Date().toISOString();
      
      // Write updated conversation back to file
      fs.writeFileSync(filePath, JSON.stringify(conversation, null, 2), 'utf-8');
      console.log(`Renamed conversation ${conversationId} to: ${newTitle}`);
      
      return true;
    } catch (error) {
      console.error('Error renaming conversation:', error);
      return false;
    }
  }
  
  /**
   * Deletes a conversation
   * @param conversationId The ID of the conversation to delete
   * @returns true if successful, false otherwise
   */
  public deleteConversation(conversationId: string): boolean {
    try {
      const filePath = path.join(this.conversationsDir, `${conversationId}.json`);
      
      if (!fs.existsSync(filePath)) {
        console.error(`Conversation file not found: ${filePath}`);
        return false;
      }
      
      // Delete the file
      fs.unlinkSync(filePath);
      console.log(`Deleted conversation: ${conversationId}`);
      
      // If the deleted conversation was the current one, create a new conversation
      if (conversationId === this.currentConversationId) {
        this.newConversation();
      }
      
      return true;
    } catch (error) {
      console.error('Error deleting conversation:', error);
      return false;
    }
  }
}
