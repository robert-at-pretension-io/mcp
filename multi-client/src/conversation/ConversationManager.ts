import { v4 as uuidv4 } from 'uuid'; // Keep UUID import for new conversation IDs
import type { IAiClient } from '../ai/IAiClient.js';
import type { ServerManager } from '../ServerManager.js';
import { ConversationState } from './ConversationState.js';
import { HumanMessage, AIMessage, SystemMessage, ToolMessage } from './Message.js';
import type { ConversationMessage } from './Message.js';
import type { Tool } from '@modelcontextprotocol/sdk/types.js'; // MCP Tool type
import { ToolParser } from './ToolParser.js'; // ParsedToolCall import removed as it's internal to ToolParser now
import { AiClientFactory } from '../ai/AiClientFactory.js';
import type { AiProviderConfig, ProviderModelsStructure } from '../types.js';
import { ConversationPersistenceService, type SerializedConversation } from './persistence/ConversationPersistenceService.js';
import { PromptFactory } from './prompts/PromptFactory.js';
import { ToolExecutor, type ToolCallRequest } from './execution/ToolExecutor.js';
import { VerificationService, type VerificationResult } from './verification/VerificationService.js'; // Import VerificationService

// __filename, __dirname, baseDir removed

// SerializedConversation interface moved to persistence service

export class ConversationManager {
  private state: ConversationState;
  private aiClient: IAiClient;
  private serverManager: ServerManager;
  private persistenceService: ConversationPersistenceService;
  private promptFactory: typeof PromptFactory; // Store static class/type
  private toolExecutor: ToolExecutor;
  private verificationService: VerificationService; // Add VerificationService instance
  private allTools: Tool[] = []; // Cache of all available tools
  private toolsLastUpdated: number = 0; // Timestamp of when tools were last updated
  private readonly TOOLS_CACHE_TTL_MS = 10 * 60 * 1000; // 10 minutes cache TTL
  private aiClientFactory: typeof AiClientFactory; // Store the factory reference for switching models

  // Persistence properties removed (handled by persistenceService)
  private currentConversationId: string; // Keep track of the active conversation ID

  // Prompts removed (handled by promptFactory)

  constructor(
    aiClient: IAiClient,
    serverManager: ServerManager,
    persistenceService: ConversationPersistenceService,
    toolExecutor: ToolExecutor,
    verificationService: VerificationService // Inject VerificationService
  ) {
    this.aiClient = aiClient;
    this.serverManager = serverManager;
    this.persistenceService = persistenceService;
    this.toolExecutor = toolExecutor;
    this.verificationService = verificationService; // Store VerificationService instance
    this.promptFactory = PromptFactory; // Assign static class
    this.state = new ConversationState(); // Initialize with no system prompt initially

    // Store the factory reference for switching models
    this.aiClientFactory = AiClientFactory;

    // Generate a new conversation ID for this session
    // TODO: Consider loading the last active conversation instead?
    this.currentConversationId = uuidv4(); // Keep uuid import for this

    // Immediately fetch all tools to prime the cache
    this.refreshToolsCache().catch(err => {
      console.warn('Failed to initialize tools cache:', err);
    });
  }

  // ensureConversationsDir removed (handled by persistenceService)

  /**
   * Saves the current conversation state using the persistence service.
   */
  private saveConversation(): void {
    this.persistenceService.saveConversation(
        this.currentConversationId,
        this.state,
        this.getAiClientModelName(),
        this.getAiProviderName()
    );
  }

  /**
   * Loads a conversation from the persistence service and updates the state.
   * @param conversationId The ID of the conversation to load.
   * @returns true if successful, false otherwise.
   */
  public loadConversation(conversationId: string): boolean {
    const loadedData = this.persistenceService.loadConversation(conversationId);
    if (!loadedData) {
        return false;
    }

    try {
        // Clear current state before loading
        this.state.clearHistory();
        this.currentConversationId = loadedData.id;

        // Reconstruct messages from loaded data
        for (const msgData of loadedData.messages) {
            let message: ConversationMessage | null = null;
            let content: string | Record<string, any>[] = msgData.content;

            // Attempt to parse content if it looks like JSON (for complex AI/Tool messages)
            if (typeof content === 'string' && (content.startsWith('{') || content.startsWith('['))) {
                try {
                    content = JSON.parse(content);
                } catch {
                    // Ignore parsing error, keep as string
                }
            }

            switch (msgData.role) {
                case 'system':
                    // System prompt is handled separately by setSystemPrompt
                    // If we load a persisted system prompt, set it here
                    if (typeof content === 'string') {
                        this.state.setSystemPrompt(content);
                    } else {
                         console.warn(`[ConversationManager] Skipping non-string system message content during load: ${JSON.stringify(content)}`);
                    }
                    continue; // Don't add system message to history array directly
                case 'human':
                    // Human message content should always be string according to LangChain types
                    if (typeof content === 'string') {
                        message = new HumanMessage(content);
                    } else {
                         console.warn(`[ConversationManager] Skipping non-string human message content during load: ${JSON.stringify(content)}`);
                    }
                    break;
                case 'ai':
                    // AIMessage constructor handles string or array content
                    message = new AIMessage(content, {
                        hasToolCalls: msgData.hasToolCalls,
                        pendingToolCalls: msgData.pendingToolCalls,
                        additional_kwargs: msgData.additional_kwargs,
                        tool_calls: msgData.tool_calls
                    });
                    break;
                case 'tool':
                     // ToolMessage content should be string
                     if (typeof content === 'string' && msgData.tool_call_id) {
                        message = new ToolMessage(
                            content,
                            msgData.tool_call_id,
                            msgData.name // Optional tool name
                        );
                    } else {
                         console.warn(`[ConversationManager] Skipping invalid tool message during load:`, msgData);
                    }
                    break;
                default:
                     console.warn(`[ConversationManager] Skipping unknown message role during load: ${msgData.role}`);
            }

            if (message) {
                this.state.addMessage(message); // Add directly to history array
            }
        }

        console.log(`[ConversationManager] Loaded conversation: ${loadedData.title}`);
        return true;
    } catch (error) {
        console.error('[ConversationManager] Error reconstructing conversation state:', error);
        // Clear potentially corrupted state
        this.state.clearHistory();
        this.currentConversationId = uuidv4(); // Reset to new ID
        return false;
    }
  }

  // listConversations implementation removed (delegated below)

  /**
   * Creates a new empty conversation, clearing state and generating a new ID.
   */
  public newConversation(): void {
    this.state.clearHistory();
    this.currentConversationId = uuidv4(); // Generate new ID using uuid
    console.log(`[ConversationManager] Created new conversation with ID: ${this.currentConversationId}`);
    // Initial save might not be necessary until first message
  }

  // renameConversation implementation removed (delegated below)
  // deleteConversation implementation removed (delegated below)


  /**
   * Switch the AI client to a different provider and model.
   * @param providerConfig The provider configuration to use.
   * @param providerModels Available models for providers (needed by factory).
   * @returns The actual model name used by the new client.
   */
  public switchAiClient(
    providerConfig: AiProviderConfig,
    providerModels: ProviderModelsStructure
  ): string { // No longer needs to be async if getAllTools is sync or cached effectively
    try {
      // Get tools (potentially from cache)
      // Assuming getAllTools is efficient enough to call here,
      // otherwise, the factory might need to fetch them if stale.
      const tools = this.allTools; // Use cached tools for now

      // Create the new client using the factory
      const newClient = this.aiClientFactory.createClient(providerConfig, providerModels, tools);

      this.aiClient = newClient;

      // Clear conversation history and start a new conversation ID on model switch
      this.newConversation(); // This clears state and sets a new ID

      const modelName = newClient.getModelName();
      console.log(`[ConversationManager] Switched AI client to: ${providerConfig.provider} (${modelName})`);
      return modelName;
    } catch (error) {
      console.error(`[ConversationManager] Failed to switch AI client:`, error instanceof Error ? error.message : String(error));
      // Optionally revert to the old client if needed, but for now, just throw
      throw error; // Re-throw to let the caller handle it (e.g., UI feedback)
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
      console.log(`[ConversationManager] Refreshed tools cache: ${this.allTools.length} tools available.`);
      return this.allTools;
    } catch (error) {
      console.error('[ConversationManager] Error refreshing tools cache:', error);
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

  // generateToolSystemPrompt removed (handled by promptFactory)

  // executeToolCalls removed (handled by toolExecutor)

  /**
   * Creates a message to send to the AI with tool results.
   * @param toolResults Map of tool call IDs to results.
   * @returns Human-readable message for the AI.
   */
  private createToolResultsMessage(toolResults: Map<string, string>): string {
    // Use the prompt from the factory
    let message = this.promptFactory.TOOL_RESULTS_PROMPT;
    // Note: The current prompt doesn't actually use the toolResults map directly.
    // It assumes the ToolMessages are already in the history.
    // If the prompt needed to dynamically include results, this method would format them.
    // For now, just returning the static prompt is correct based on its content.
    return message;
  }

  // generateVerificationCriteria removed (moved to VerificationService)
  // verifyResponse removed (moved to VerificationService)

  /**
   * Processes a user's message, interacts with the AI, handles tool calls, and performs verification.
   * @param userInput - The text input from the user.
   * @returns The AI's final response content for this turn as a string.
   */
  async processUserMessage(userInput: string): Promise<string> {
    console.log(`[ConversationManager] Processing user message: "${userInput}"`);

    // 1. Add user message and save
    this.state.addMessage(new HumanMessage(userInput));
    this.saveConversation();

    // 2. Prepare for AI call (criteria, system prompt, compaction)
    await this._prepareForAiCall(userInput);

    // 3. Initial AI call
    let currentResponseContent: string;
    try {
        currentResponseContent = await this._makeAiCall();
    } catch (error) {
        console.error("[ConversationManager] Error during initial AI interaction:", error);
        return `Sorry, I encountered an error: ${error instanceof Error ? error.message : String(error)}`;
    }

    // 4. Handle Tool Calls (Loop)
    currentResponseContent = await this._handleToolLoop(currentResponseContent);

    // 5. Handle Verification and Correction
    let finalResponseContent = await this._handleVerification(currentResponseContent);

    // 6. Add final AI response to state and save
    const finalAiMessage = new AIMessage(finalResponseContent, { hasToolCalls: false });
    this.state.addMessage(finalAiMessage);
    this.saveConversation();

    // 7. Return the final response content string
    return finalResponseContent;
  }

  /** Prepares the conversation state for an AI call (criteria, system prompt, compaction). */
  private async _prepareForAiCall(userInput: string): Promise<void> {
      // Generate verification criteria if needed
      if (!this.state.getVerificationState()) {
          const criteria = await this.verificationService.generateVerificationCriteria(userInput);
          this.state.setVerificationState(userInput, criteria);
      }

      // Generate the dynamic system prompt
      const tools = await this.getAllTools();
      const systemPrompt = this.promptFactory.createToolSystemPrompt(tools);
      this.state.setSystemPrompt(systemPrompt);

      // Compact history if needed
      const messages = this.state.getMessages();
      if (messages.length > 20) { // Example threshold
          await this.state.compactHistory(this.promptFactory.CONVERSATION_COMPACTION_PROMPT, this.aiClient);
          console.log(`[ConversationManager] History compacted. Current length: ${this.state.getMessages().length}`);
      }
  }

  /** Makes a call to the AI client with the current conversation history. */
  private async _makeAiCall(): Promise<string> {
      const messagesForAi = this.state.getMessages();
      const responseContent = await this.aiClient.generateResponse(messagesForAi);
      console.log(`[ConversationManager] AI Response (${this.aiClient.getModelName()}):`, responseContent.substring(0, 200) + (responseContent.length > 200 ? '...' : ''));
      return responseContent;
  }

  /** Handles the loop of detecting AI tool calls, executing them, and getting follow-up responses. */
  private async _handleToolLoop(initialResponseContent: string): Promise<string> {
      let currentResponseContent = initialResponseContent;
      let toolRound = 0;
      const maxToolRounds = 5; // Limit recursive tool calls

      while (toolRound < maxToolRounds) {
          toolRound++;
          console.log(`[ConversationManager] --- Tool/Response Round ${toolRound} ---`);

          // --- Check for BOTH standard and MCP-style tool calls ---
          // Create the AIMessage object representing the AI's request for this round
          const aiMessageRequestingTools = new AIMessage(currentResponseContent);
          const standardToolCalls = (aiMessageRequestingTools.tool_calls || [])
              .filter((tc): tc is { id: string; name: string; args: Record<string, any> } => tc.id !== undefined);
          const mcpToolCalls = ToolParser.parseToolCalls(typeof currentResponseContent === 'string' ? currentResponseContent : JSON.stringify(currentResponseContent));

          if (standardToolCalls.length === 0 && mcpToolCalls.length === 0) {
              console.log("[ConversationManager] No tool calls (standard or MCP) found in AI response. Exiting tool loop.");
              break; // Exit loop if neither type is found
          }

          // Add the AI message *requesting* the tools to history
          // We will modify this specific object later
          aiMessageRequestingTools.hasToolCalls = true; // Mark that a request was made
          aiMessageRequestingTools.pendingToolCalls = true;
          this.state.addMessage(aiMessageRequestingTools); // Add the object to state
          this.saveConversation();

          let toolCallsToExecute: ToolCallRequest[] = [];
          let isUsingStandardCalls = false;

          if (standardToolCalls.length > 0) {
              // Prioritize standard calls if available
              console.log(`[ConversationManager] Found ${standardToolCalls.length} standard tool calls. Using standard mechanism.`);
              toolCallsToExecute = standardToolCalls.map(tc => ({ id: tc.id, name: tc.name, args: tc.args }));
              isUsingStandardCalls = true;
          } else {
              // Fallback to MCP calls if no standard calls found
              console.log(`[ConversationManager] Found ${mcpToolCalls.length} MCP-style tool calls. Using manual result formatting.`);
              toolCallsToExecute = mcpToolCalls.map(call => ({
                  id: `mcpcall-${Date.now()}-${Math.floor(Math.random() * 1000)}`, // Generate ID
                  name: call.name,
                  args: call.arguments
              }));
              isUsingStandardCalls = false;
          }

          // Execute tools
          const toolResultsMap = await this.toolExecutor.executeToolCalls(toolCallsToExecute);
          // Now update the *specific message object* we added earlier
          aiMessageRequestingTools.pendingToolCalls = false; // Mark as done

          // --- Add results back to history ---
          if (isUsingStandardCalls) {
              // Use standard ToolMessage for standard calls
              console.log("[ConversationManager] Adding results using standard ToolMessage.");
              for (const executedCall of toolCallsToExecute) {
                  const result = toolResultsMap.get(executedCall.id) || `Error: Result not found for tool call ${executedCall.id}`;
                  this.state.addMessage(new ToolMessage(result, executedCall.id, executedCall.name));
              }
          } else {
              // Format results into a string and add as a *new* AIMessage for MCP calls
              console.log("[ConversationManager] Adding results as a formatted AIMessage.");
              let resultsString = "Tool results received:\n";
              for (const executedCall of toolCallsToExecute) {
                  const result = toolResultsMap.get(executedCall.id) || `Error: Result not found for tool call ${executedCall.id}`;
                  resultsString += `\n--- Tool: ${executedCall.name} ---\n`;
                  resultsString += result;
                  resultsString += `\n--- End Tool: ${executedCall.name} ---\n`;
              }
              // Add this formatted string as a new AI message turn
              this.state.addMessage(new AIMessage(resultsString.trim()));
          }
          this.saveConversation(); // Save after adding results

          // Make follow-up AI call
          try {
              currentResponseContent = await this._makeAiCall();
          } catch (error) {
              console.error(`[ConversationManager] Error during AI follow-up interaction (Round ${toolRound + 1}):`, error);
              const errorMessage = `Sorry, I encountered an error processing the tool results: ${error instanceof Error ? error.message : String(error)}`;
              this.state.addMessage(new AIMessage(errorMessage));
              this.saveConversation();
              currentResponseContent = errorMessage;
              break; // Exit loop on error
          }
      } // End while loop

      if (toolRound >= maxToolRounds) {
          console.warn(`[ConversationManager] Reached maximum tool call rounds (${maxToolRounds}). Proceeding with last response.`);
      }

      return currentResponseContent; // Return the last response content after the loop
  }

   /** Handles the verification process and potential correction call. */
   private async _handleVerification(responseContent: string): Promise<string> {
       const verificationState = this.state.getVerificationState();
       let finalResponseContent = responseContent;

       if (verificationState) {
           const { originalRequest, criteria } = verificationState;
           const relevantSequence = this.state.getRelevantSequenceForVerification();
           const verificationResult = await this.verificationService.verifyResponse(
               originalRequest,
               criteria,
               relevantSequence + `\n\nAssistant: ${finalResponseContent}` // Append final response for verification
           );

           // TODO: Attach verificationResult to the final AI message if needed for UI

           if (!verificationResult.passes) {
               console.log('[ConversationManager] Response verification failed. Retrying with feedback.');
               try {
                   // Pass the history *including* the failed response for context
                   const historyForCorrection = this.state.getMessages();
                   const correctedResponseContent = await this.verificationService.generateCorrectedResponse(
                       historyForCorrection,
                       finalResponseContent, // Pass the failed content
                       verificationResult.feedback
                   );

                   // --- Check if corrected response has tool calls ---
                   const correctedAiMessage = new AIMessage(correctedResponseContent);
                   const correctedToolCalls = (correctedAiMessage.tool_calls || [])
                       .filter((tc): tc is { id: string; name: string; args: Record<string, any> } => tc.id !== undefined);

                   if (correctedToolCalls.length > 0) {
                       console.log(`[ConversationManager] Corrected response contains ${correctedToolCalls.length} tool calls. Executing them.`);

                       // Add the corrected AI message (requesting tools) to history
                       correctedAiMessage.hasToolCalls = true;
                       correctedAiMessage.pendingToolCalls = true; // Will be set to false shortly
                       this.state.addMessage(correctedAiMessage);
                       this.saveConversation();

                       // Prepare tool calls for execution
                       const toolCallsToExecute: ToolCallRequest[] = correctedToolCalls.map(tc => ({
                           id: tc.id,
                           name: tc.name,
                           args: tc.args
                       }));

                       // Execute tools
                       const toolResultsMap = await this.toolExecutor.executeToolCalls(toolCallsToExecute);
                       correctedAiMessage.pendingToolCalls = false; // Mark as done

                       // Add tool results to history using standard ToolMessage
                       console.log("[ConversationManager] Adding results from corrected response using standard ToolMessage.");
                       for (const executedCall of toolCallsToExecute) {
                           const result = toolResultsMap.get(executedCall.id) || `Error: Result not found for tool call ${executedCall.id}`;
                           this.state.addMessage(new ToolMessage(result, executedCall.id, executedCall.name));
                       }
                       this.saveConversation(); // Save after adding results

                       // Make the final AI call after executing tools from the corrected response
                       console.log("[ConversationManager] Making final AI call after executing tools from corrected response.");
                       finalResponseContent = await this._makeAiCall(); // Update final content

                   } else {
                       // No tool calls in corrected response, just use it as the final content
                       console.log('[ConversationManager] Corrected response has no tool calls.');
                       finalResponseContent = correctedResponseContent;
                       // The main processUserMessage adds the final AIMessage at the end.
                   }
                   // --- End tool check for corrected response ---

               } catch (error) {
                   console.error('[ConversationManager] Error during verification correction phase:', error);
                   // Keep the uncorrected response if retry fails
                   // finalResponseContent remains the original responseContent before correction attempt
               }
           }
       }
       return finalResponseContent; // Return the potentially corrected (and tool-processed) content
   }


  /**
   * Clears the conversation history and starts a new conversation ID.
   */
  public clearConversation(): void {
      this.newConversation(); // Use newConversation which handles state clearing and ID generation
      console.log("[ConversationManager] Conversation history cleared.");
  }


  /**
   * Gets the current conversation history (including system prompt).
   */
  public getHistory(): ConversationMessage[] {
    return this.state.getMessages();
  }

  /**
   * Gets metadata for the currently active conversation.
   * Reads from persistence or returns default if not yet saved.
   */
  public getCurrentConversation(): Omit<SerializedConversation, 'messages'> {
      const savedData = this.persistenceService.loadConversation(this.currentConversationId);

      if (savedData) {
          // Return metadata from the loaded file, excluding messages
          const { messages, ...metadata } = savedData;
          return metadata;
      } else {
          // Return default metadata for a new, unsaved conversation
          return {
              id: this.currentConversationId,
              title: 'New Conversation',
              modelName: this.getAiClientModelName(),
              provider: this.getAiProviderName(),
              createdAt: new Date().toISOString(), // Placeholder
              updatedAt: new Date().toISOString(), // Placeholder
              // messageCount: this.state.getMessages().length // Can add if needed
          };
      }
  }

  // --- Persistence Method Delegation ---
  // Expose persistence methods, delegating to the service

  /**
   * Lists all saved conversations using the persistence service.
   * Adds `isActive` flag.
   */
  public listConversations(): (Omit<SerializedConversation, 'messages'> & { isActive: boolean })[] {
      const listedConvos = this.persistenceService.listConversations();
      return listedConvos.map(convo => ({
          ...convo,
          isActive: convo.id === this.currentConversationId
      }));
  }

  /**
   * Renames a conversation using the persistence service.
   */
  public renameConversation(conversationId: string, newTitle: string): boolean {
      return this.persistenceService.renameConversation(conversationId, newTitle);
  }

  /**
   * Deletes a conversation using the persistence service.
   * If the deleted conversation was the current one, creates a new conversation.
   */
  public deleteConversation(conversationId: string): boolean {
      const success = this.persistenceService.deleteConversation(conversationId);
      if (success && conversationId === this.currentConversationId) {
          this.newConversation(); // Start a new one if the current was deleted
      }
      return success;
  }

} // End ConversationManager class
