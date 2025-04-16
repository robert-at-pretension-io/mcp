import type { IAiClient } from '../ai/IAiClient.js';
import type { ServerManager } from '../ServerManager.js';
import type { ConversationMessage } from './Message.js';
import type { AiProviderConfig, ProviderModelsStructure } from '../types.js';
import { ConversationPersistenceService, type SerializedConversation } from './persistence/ConversationPersistenceService.js';
import { ToolExecutor } from './execution/ToolExecutor.js';
import { VerificationService } from './verification/VerificationService.js';
export declare class ConversationManager {
    private state;
    private aiClient;
    private serverManager;
    private persistenceService;
    private promptFactory;
    private toolExecutor;
    private verificationService;
    private allTools;
    private toolsLastUpdated;
    private readonly TOOLS_CACHE_TTL_MS;
    private aiClientFactory;
    private currentConversationId;
    constructor(aiClient: IAiClient, serverManager: ServerManager, persistenceService: ConversationPersistenceService, toolExecutor: ToolExecutor, verificationService: VerificationService);
    /**
     * Saves the current conversation state using the persistence service.
     */
    private saveConversation;
    /**
     * Loads a conversation from the persistence service and updates the state.
     * @param conversationId The ID of the conversation to load.
     * @returns true if successful, false otherwise.
     */
    loadConversation(conversationId: string): boolean;
    /**
     * Creates a new empty conversation, clearing state and generating a new ID.
     */
    newConversation(): void;
    /**
     * Switch the AI client to a different provider and model.
     * @param providerConfig The provider configuration to use.
     * @param providerModels Available models for providers (needed by factory).
     * @returns The actual model name used by the new client.
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
     * Creates a message to send to the AI with tool results.
     * @param toolResults Map of tool call IDs to results.
     * @returns Human-readable message for the AI.
     */
    private createToolResultsMessage;
    /**
     * Processes a user's message, interacts with the AI, handles tool calls, and performs verification.
     * @param userInput - The text input from the user.
     * @returns The AI's final response content for this turn as a string.
     */
    processUserMessage(userInput: string): Promise<string>;
    /** Prepares the conversation state for an AI call (criteria, system prompt, compaction). */
    private _prepareForAiCall;
    /** Makes a call to the AI client with the current conversation history. */
    private _makeAiCall;
    /** Handles the loop of detecting AI tool calls, executing them, and getting follow-up responses. */
    private _handleToolLoop;
    /** Handles the verification process and potential correction call. */
    private _handleVerification;
    /**
     * Clears the conversation history and starts a new conversation ID.
     */
    clearConversation(): void;
    /**
     * Gets the current conversation history (including system prompt).
     */
    getHistory(): ConversationMessage[];
    /**
     * Gets metadata for the currently active conversation.
     * Reads from persistence or returns default if not yet saved.
     */
    getCurrentConversation(): Omit<SerializedConversation, 'messages'>;
    /**
     * Lists all saved conversations using the persistence service.
     * Adds `isActive` flag.
     */
    listConversations(): (Omit<SerializedConversation, 'messages'> & {
        isActive: boolean;
    })[];
    /**
     * Renames a conversation using the persistence service.
     */
    renameConversation(conversationId: string, newTitle: string): boolean;
    /**
     * Deletes a conversation using the persistence service.
     * If the deleted conversation was the current one, creates a new conversation.
     */
    deleteConversation(conversationId: string): boolean;
}
