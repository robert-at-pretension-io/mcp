import type { IAiClient } from '../ai/IAiClient.js';
import type { ServerManager } from '../ServerManager.js';
import type { ConversationMessage } from './Message.js';
export declare class ConversationManager {
    private state;
    private aiClient;
    private serverManager;
    private allTools;
    private toolsLastUpdated;
    private readonly TOOLS_CACHE_TTL_MS;
    private readonly TOOL_RESULTS_PROMPT;
    private readonly INVALID_TOOL_FORMAT_PROMPT;
    private readonly VERIFICATION_CRITERIA_PROMPT;
    private readonly VERIFICATION_PROMPT;
    private readonly VERIFICATION_FAILURE_PROMPT;
    private readonly CONVERSATION_COMPACTION_PROMPT;
    constructor(aiClient: IAiClient, serverManager: ServerManager);
    /**
     * Gets the model name identifier from the underlying AI client.
     */
    getAiClientModelName(): string;
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
     * Executes a set of parsed tool calls in parallel.
     * @param toolCalls Array of parsed tool calls to execute.
     * @returns Promise that resolves to an array of tool execution results.
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
     * Clears the conversation history.
     */
    clearConversation(): void;
    /**
     * Gets the current conversation history.
     */
    getHistory(): ConversationMessage[];
}
