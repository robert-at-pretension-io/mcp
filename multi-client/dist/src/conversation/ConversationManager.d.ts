import type { IAiClient } from '../ai/IAiClient.js';
import type { ServerManager } from '../ServerManager.js';
import type { AiProviderConfig, ProviderModelsStructure } from '../types.js';
export declare class ConversationManager {
    private state;
    private aiClient;
    private serverManager;
    private allTools;
    private toolsLastUpdated;
    private readonly TOOLS_CACHE_TTL_MS;
    private aiClientFactory;
    private conversationsDir;
    private currentConversationId;
    private saveDebounceTimeout;
    private readonly TOOL_RESULTS_PROMPT;
    private readonly INVALID_TOOL_FORMAT_PROMPT;
    private readonly VERIFICATION_CRITERIA_PROMPT;
    private readonly VERIFICATION_PROMPT;
    private readonly VERIFICATION_FAILURE_PROMPT;
    private readonly CONVERSATION_COMPACTION_PROMPT;
    constructor(aiClient: IAiClient, serverManager: ServerManager, providerModels: ProviderModelsStructure);
    /**
     * Ensures the conversations directory exists
     */
    private ensureConversationsDir;
    /**
     * Saves the current conversation to disk
     */
    private saveConversation;
    /**
     * Loads a conversation from disk
     * @param conversationId The ID of the conversation to load
     * @returns true if successful, false otherwise
     */
    loadConversation(conversationId: string): boolean;
    /**
     * Lists all saved conversations
     * @returns Array of conversation metadata
     */
    listConversations(): any[];
    /**
     * Creates a new empty conversation
     */
    newConversation(): void;
    /**
     * Switch the AI client to a different provider and model
     * @param providerConfig The provider configuration to use
     * @param providerModels Available models for providers
     * @param providerModels Available models for providers
     * @returns The new model name if switch was successful
     */
    switchAiClient(providerConfig: AiProviderConfig, providerModels: ProviderModelsStructure): string;
    /**
     * Gets the model name identifier from the underlying AI client.
     */
    getAiClientModelName(): string;
    /**
     * Gets the provider name identifier from the underlying AI client.
     */
    getAiProviderName(): string;
    /**
     * Refreshes the tools cache by fetching all tools from connected servers.
     * @returns Promise that resolves when the cache is refreshed.
     */
    private refreshToolsCache;
    /**
     * Gets all available tools, refreshing the cache if necessary.
     * @returns Promise that resolves to an array of all available tools.
     */
    private getAllTools;
    /**
     * Generates the system prompt including tool definitions.
     */
    private generateToolSystemPrompt;
    /**
     * Executes a set of parsed tool calls (now including generated IDs) in parallel.
     * Executes a set of tool calls provided by the AI (using LangChain's standard format).
     * @param toolCallsFromAI Array of tool calls, each including the AI-generated `id`.
     * @returns Promise that resolves to a map of tool call IDs to their string results.
     */
    private executeToolCalls;
    /**
     * Creates a message to send to the AI with tool results.
     * @param toolResults Map of tool call IDs to results.
     * @returns Human-readable message for the AI.
     */
    private createToolResultsMessage;
    /**
     * Generates verification criteria for a user request
     * @param userInput The original user input/request
     * @returns The generated verification criteria
     */
    private generateVerificationCriteria;
    /**
     * Verifies an AI response against the criteria
     * @param originalRequest The original user request
     * @param criteria The verification criteria
     * @param relevantSequence The formatted conversation sequence to verify
     * @returns Object with verification result (passes) and feedback
     */
    private verifyResponse;
    /**
     * Processes a user's message, interacts with the AI, and potentially handles tool calls.
     * @param userInput - The text input from the user.
     * @returns The AI's final response for this turn.
     */
    processUserMessage(userInput: string): Promise<string>;
    /**
     * Renames a conversation
     * @param conversationId The ID of the conversation to rename
     * @param newTitle The new title for the conversation
     * @returns true if successful, false otherwise
     */
    renameConversation(conversationId: string, newTitle: string): boolean;
    /**
     * Deletes a conversation
     * @param conversationId The ID of the conversation to delete
     * @returns true if successful, false otherwise
     */
    deleteConversation(conversationId: string): boolean;
}
