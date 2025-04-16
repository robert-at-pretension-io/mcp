import * as fs from 'node:fs';
import * as path from 'node:path';
import type { ConversationState } from '../ConversationState.js';
import type { ConversationMessage } from '../Message.js'; // Assuming Message types are needed for serialization structure

// Interface for serialized conversations (can be moved to types.ts later)
export interface SerializedConversation {
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
    // Add other potential fields from LangChain messages if needed for restoration
    name?: string; // For ToolMessage
    tool_call_id?: string; // For ToolMessage
    additional_kwargs?: Record<string, any>; // For AIMessage
    tool_calls?: any[]; // For AIMessage
  }[];
}


export class ConversationPersistenceService {
    private conversationsDir: string;
    // Use a Map to store debounce timeouts per conversation ID
    private saveDebounceTimeouts: Map<string, NodeJS.Timeout> = new Map();
    private readonly SAVE_DEBOUNCE_MS = 1000; // 1 second debounce

    constructor(baseDir: string) {
        this.conversationsDir = path.join(baseDir, 'conversations');
        this.ensureConversationsDir();
    }

    /**
     * Ensures the conversations directory exists
     */
    private ensureConversationsDir(): void {
        try {
            if (!fs.existsSync(this.conversationsDir)) {
                fs.mkdirSync(this.conversationsDir, { recursive: true });
                console.log(`[Persistence] Created conversations directory at: ${this.conversationsDir}`);
            }
        } catch (error) {
            console.error(`[Persistence] Error creating conversations directory:`, error);
            // Depending on requirements, might want to throw or handle differently
        }
    }

    /**
     * Saves the current conversation state to disk, debounced.
     * @param conversationId The ID of the conversation to save.
     * @param state The current ConversationState.
     * @param modelName The name of the AI model used.
     * @param provider The name of the AI provider used.
     */
    public saveConversation(
        conversationId: string,
        state: ConversationState,
        modelName: string,
        provider: string
    ): void {
        // Clear existing timeout for this specific conversation ID
        const existingTimeout = this.saveDebounceTimeouts.get(conversationId);
        if (existingTimeout) {
            clearTimeout(existingTimeout);
        }

        // Set a new debounce timeout for this conversation ID
        const newTimeout = setTimeout(() => {
            this._performSave(conversationId, state, modelName, provider);
            // Remove the timeout entry after execution
            this.saveDebounceTimeouts.delete(conversationId);
        }, this.SAVE_DEBOUNCE_MS);

        // Store the new timeout
        this.saveDebounceTimeouts.set(conversationId, newTimeout);
    }

    /**
     * Performs the actual saving logic.
     */
    private _performSave(
        conversationId: string,
        state: ConversationState,
        modelName: string,
        provider: string
    ): void {
         try {
            const messages = state.getMessages(); // Get all messages including system

            // Don't save if there are no non-system messages
            if (state.getHistoryWithoutSystemPrompt().length === 0) {
                console.log(`[Persistence] Skipping save for empty conversation: ${conversationId}`);
                return;
            }

            // Generate a title from the first few user messages
            let title = 'New Conversation';
            const userMessages = state.getHistoryWithoutSystemPrompt().filter(m => m._getType() === 'human');

            if (userMessages.length > 0) {
                const firstMessageContent = userMessages[0].content;
                const firstMessage = typeof firstMessageContent === 'string'
                    ? firstMessageContent
                    : JSON.stringify(firstMessageContent); // Handle complex content
                title = firstMessage.length > 50
                    ? firstMessage.substring(0, 47) + '...'
                    : firstMessage;
            }

            // Check if file exists to get createdAt time
            const filePath = path.join(this.conversationsDir, `${conversationId}.json`);
            let createdAt = new Date().toISOString();
            if (fs.existsSync(filePath)) {
                try {
                    const existingData = fs.readFileSync(filePath, 'utf-8');
                    const existingConv = JSON.parse(existingData) as SerializedConversation;
                    createdAt = existingConv.createdAt || createdAt; // Use existing createdAt if available
                } catch (readError) {
                    console.warn(`[Persistence] Could not read existing file to preserve createdAt for ${conversationId}:`, readError);
                }
            }


            // Create serialized conversation
            const conversation: SerializedConversation = {
                id: conversationId,
                title,
                modelName: modelName,
                provider: provider,
                createdAt: createdAt, // Preserve or set new
                updatedAt: new Date().toISOString(), // Always update timestamp
                messages: messages.map(msg => ({ // Serialize all messages
                    role: msg._getType(),
                    content: typeof msg.content === 'string' ? msg.content : JSON.stringify(msg.content), // Ensure content is stringified if complex
                    // Include other relevant properties from LangChain message types
                    hasToolCalls: (msg as any).hasToolCalls,
                    pendingToolCalls: (msg as any).pendingToolCalls,
                    name: (msg as any).name, // For ToolMessage
                    tool_call_id: (msg as any).tool_call_id, // For ToolMessage
                    additional_kwargs: (msg as any).additional_kwargs, // For AIMessage
                    tool_calls: (msg as any).tool_calls // For AIMessage
                }))
            };

            // Write to file
            fs.writeFileSync(filePath, JSON.stringify(conversation, null, 2), 'utf-8');
            console.log(`[Persistence] Saved conversation to: ${filePath}`);
        } catch (error) {
            console.error('[Persistence] Error saving conversation:', error);
        }
        // Timeout removal is handled in the setTimeout callback in saveConversation
    }


    /**
     * Loads a conversation from disk.
     * @param conversationId The ID of the conversation to load.
     * @returns The raw SerializedConversation data or null if loading fails.
     */
    public loadConversation(conversationId: string): SerializedConversation | null {
        try {
            const filePath = path.join(this.conversationsDir, `${conversationId}.json`);

            if (!fs.existsSync(filePath)) {
                console.error(`[Persistence] Conversation file not found: ${filePath}`);
                return null;
            }

            const conversationData = fs.readFileSync(filePath, 'utf-8');
            const conversation: SerializedConversation = JSON.parse(conversationData);

            console.log(`[Persistence] Loaded conversation data: ${conversation.title} (${conversationId})`);
            return conversation; // Return the raw data

        } catch (error) {
            console.error('[Persistence] Error loading conversation:', error);
            return null;
        }
    }

    /**
     * Lists metadata for all saved conversations.
     * @returns Array of conversation metadata (excluding messages).
     */
    public listConversations(): Omit<SerializedConversation, 'messages'>[] {
        try {
            this.ensureConversationsDir(); // Ensure directory exists before reading

            const files = fs.readdirSync(this.conversationsDir)
                .filter(file => file.endsWith('.json'));

            const conversations = files.map(file => {
                try {
                    const filePath = path.join(this.conversationsDir, file);
                    const data = fs.readFileSync(filePath, 'utf-8');
                    const conversation: SerializedConversation = JSON.parse(data);

                    // Return metadata only
                    const { messages, ...metadata } = conversation; // Destructure to exclude messages
                    return metadata;
                } catch (error) {
                    console.warn(`[Persistence] Error parsing conversation file metadata: ${file}`, error);
                    return null;
                }
            }).filter((c): c is Omit<SerializedConversation, 'messages'> => c !== null); // Type assertion and filter nulls

            // Sort by updatedAt, most recent first
            return conversations.sort((a, b) =>
                new Date(b.updatedAt || 0).getTime() - new Date(a.updatedAt || 0).getTime()
            );
        } catch (error) {
            console.error('[Persistence] Error listing conversations:', error);
            return [];
        }
    }

    /**
     * Renames a conversation file by updating its title metadata.
     * @param conversationId The ID of the conversation to rename.
     * @param newTitle The new title for the conversation.
     * @returns true if successful, false otherwise.
     */
    public renameConversation(conversationId: string, newTitle: string): boolean {
        try {
            const filePath = path.join(this.conversationsDir, `${conversationId}.json`);

            if (!fs.existsSync(filePath)) {
                console.error(`[Persistence] Conversation file not found for rename: ${filePath}`);
                return false;
            }

            const conversationData = fs.readFileSync(filePath, 'utf-8');
            const conversation: SerializedConversation = JSON.parse(conversationData);

            // Update title and timestamp
            conversation.title = newTitle;
            conversation.updatedAt = new Date().toISOString();

            // Write updated conversation back to file
            fs.writeFileSync(filePath, JSON.stringify(conversation, null, 2), 'utf-8');
            console.log(`[Persistence] Renamed conversation ${conversationId} to: ${newTitle}`);

            return true;
        } catch (error) {
            console.error('[Persistence] Error renaming conversation:', error);
            return false;
        }
    }

    /**
     * Deletes a conversation file.
     * @param conversationId The ID of the conversation to delete.
     * @returns true if successful, false otherwise.
     */
    public deleteConversation(conversationId: string): boolean {
        try {
            const filePath = path.join(this.conversationsDir, `${conversationId}.json`);

            if (!fs.existsSync(filePath)) {
                // If file doesn't exist, consider it successfully "deleted"
                console.warn(`[Persistence] Conversation file not found for deletion (already deleted?): ${filePath}`);
                return true;
            }

            // Delete the file
            fs.unlinkSync(filePath);
            console.log(`[Persistence] Deleted conversation file: ${conversationId}.json`);

            return true;
        } catch (error) {
            console.error('[Persistence] Error deleting conversation:', error);
            return false;
        }
    }
}
