import { create } from 'zustand';
import { immer, type WritableDraft } from 'zustand/middleware/immer'; // Import WritableDraft
import { devtools } from 'zustand/middleware';
import {
    fetchConversationsListApi,
    fetchProvidersApi,
    switchProviderAndModelApi,
    updateApiKeyApi,
    fetchServerConfigApi,
    saveServerConfigurationsApi,
    // Import other API functions as needed
} from '@/services/api';
import { Socket } from 'socket.io-client'; // Import socket type if needed for actions

// --- Types ---

// Based on multi-client/src/conversation/Message.ts and socket events
export interface Message {
    id?: string; // Optional ID, might be added by backend or frontend
    role: 'human' | 'ai' | 'system' | 'tool' | 'error';
    content: string | Record<string, any>[]; // Content can be complex for tool calls
    hasToolCalls?: boolean;
    pendingToolCalls?: boolean;
    toolCallId?: string; // For ToolMessage
    toolName?: string; // For ToolMessage
    // Add other potential fields like timestamp if needed
}

// Based on socket 'conversations-list' event and appState.js
export interface ConversationSummary {
    id: string;
    title: string;
    createdAt: string; // ISO string
    updatedAt: string; // ISO string
    provider?: string;
    modelName?: string;
}

// Based on socket 'conversation-loaded' event
export interface Conversation extends ConversationSummary {
    messages: Message[];
}

// Based on socket 'servers-info' event and appState.js
export interface ServerInfo {
    name: string;
    status: 'connected' | 'disconnected' | 'error' | 'connecting' | 'unknown';
    // Add other potential fields like PID if provided
}

// Based on API response for /ai/providers and appState.js
export interface ProviderConfig {
    provider: string; // Added provider name for consistency
    model?: string; // Default model for the provider
    apiKeySet?: boolean; // Indicates if an API key is configured server-side
    // Add other config fields if present (e.g., baseURL)
}
export interface Providers {
    [providerName: string]: ProviderConfig;
}
export interface ProviderModels {
     [providerNameLowercase: string]: { models: string[] };
}

// Based on API response for /servers/config and appState.js
export interface ServerConfig {
    command: string;
    args?: string[];
    env?: Record<string, string>;
    // Add other fields like 'type' if present
}
export interface McpServersConfig {
    mcpServers: {
        [serverName: string]: ServerConfig;
    };
    // Include other top-level config fields if they exist (e.g., default_tool_timeout)
}

// Based on socket 'tools-info' event and appState.js
export interface ToolInfo {
    name: string;
    description?: string;
    input_schema?: any; // Adjust type based on actual schema format
}
export interface ToolsByServer {
    [serverName: string]: ToolInfo[];
}


// --- State Slice Types ---

interface UiState {
    isThinking: boolean;
    thinkingMessage: string;
    statusMessage: string;
    connectedServersText: string;
    isSidebarOpen: boolean; // For large screens toggle
    isPanelOpen: boolean; // For small screens overlay
    isPanelCollapsed: boolean; // For large screens collapsed state
    isModelModalOpen: boolean;
    isServersModalOpen: boolean;
    isConfigEditorOpen: boolean;
    currentEditingConfigFile: string | null;
}

interface ChatState {
    messages: Message[];
    currentConversationId: string | null;
}

interface ConversationListState {
    conversations: ConversationSummary[];
}

interface ProviderState {
    currentProvider: string;
    providers: Providers;
    providerModels: ProviderModels;
}

interface ServerState {
    servers: ServerInfo[]; // Live status from socket
    serverConfig: McpServersConfig; // For editing in modal
    selectedServerName: string | null; // For server modal editing
}

interface ToolState {
    allToolsData: ToolsByServer;
}

// --- Action Types ---

interface UiActions {
    setThinking: (thinking: boolean, message?: string) => void;
    setStatusMessage: (message: string) => void;
    toggleSidebar: () => void; // Handles both mobile overlay and desktop collapse
    closeSidebar: () => void; // Explicitly close mobile overlay
    openModelModal: () => void;
    closeModelModal: () => void;
    openServersModal: () => void;
    closeServersModal: () => void;
    openConfigEditor: (fileName: string) => void;
    closeConfigEditor: () => void;
}

interface ChatActions {
    setMessages: (messages: Message[]) => void;
    addMessage: (message: Message) => void;
    setCurrentConversationId: (id: string | null) => void;
    clearConversation: () => void;
}

interface ConversationListActions {
    setConversations: (conversations: ConversationSummary[]) => void;
    updateConversationInList: (conversation: ConversationSummary) => void;
    removeConversationFromList: (id: string) => void;
    fetchConversationsList: () => Promise<void>;
}

interface ProviderActions {
    setCurrentProvider: (provider: string) => void;
    setProviders: (providers: Providers) => void;
    setProviderModels: (models: ProviderModels) => void;
    fetchProviders: () => Promise<void>;
    switchProviderAndModel: (provider: string, model: string) => Promise<void>;
    updateApiKey: (provider: string, apiKey: string) => Promise<any>; // Return API response
}

interface ServerActions {
    setServersStatus: (servers: ServerInfo[]) => void;
    setServerConfig: (config: McpServersConfig) => void;
    setSelectedServerName: (name: string | null) => void;
    fetchServerConfig: () => Promise<void>;
    saveServerConfig: (config: McpServersConfig) => Promise<any>; // Return API response
}

interface ToolActions {
    setAllToolsData: (tools: ToolsByServer) => void;
}

interface SocketActions {
    setSocket: (socket: Socket | null) => void; // Allow setting the socket instance
    emitUserMessage: (message: string) => void;
    emitClearConversation: () => void;
    emitNewConversation: () => void;
    emitLoadConversation: (id: string) => void;
    // Add other emit actions if needed
}

// --- Combined Store Type ---

type AppState = UiState & ChatState & ConversationListState & ProviderState & ServerState & ToolState;
type AppActions = UiActions & ChatActions & ConversationListActions & ProviderActions & ServerActions & ToolActions & SocketActions;
export type StoreType = AppState & AppActions & { // Export StoreType
    // Combined actions like fetchInitialData
    fetchInitialData: () => Promise<void>;
    _socket: Socket | null; // Internal socket instance
};

// --- Initial State ---

const initialState: AppState = {
    // UI
    isThinking: false,
    thinkingMessage: 'AI is thinking...',
    statusMessage: 'Initializing...',
    isSidebarOpen: false, // Default closed on large screens
    isPanelOpen: false, // Default closed on small screens
    isPanelCollapsed: false, // Default not collapsed on large screens
    isModelModalOpen: false,
    isServersModalOpen: false,
    isConfigEditorOpen: false,
    currentEditingConfigFile: null,
    connectedServersText: '',
    // Chat
    messages: [],
    currentConversationId: null,
    // Conversations
    conversations: [],
    // Providers
    currentProvider: '',
    providers: {},
    providerModels: {},
    // Servers
    servers: [],
    serverConfig: { mcpServers: {} },
    selectedServerName: null,
    // Tools
    allToolsData: {},
};


// --- Store Implementation ---

export const useStore = create<StoreType>()(
    devtools(
        immer((set, get: () => StoreType) => ({ // Add type annotation for get
            ...initialState,
            _socket: null, // Initialize internal socket state

            // --- UI Actions ---
            setThinking: (thinking: boolean, message: string = 'AI is thinking...') => set((state: WritableDraft<StoreType>) => { // Use WritableDraft
                state.isThinking = thinking;
                if (thinking) {
                    state.thinkingMessage = message;
                    state.statusMessage = message; // Also update main status
                } else {
                    state.statusMessage = 'Ready'; // Reset status when not thinking
                }
            }),
            setStatusMessage: (message: string) => set({ statusMessage: message }), // Type parameter
            toggleSidebar: () => set((state: WritableDraft<StoreType>) => { // Use WritableDraft
                // Logic depends on screen size, ideally handled by CSS media queries reacting to classes
                // This toggle might control both states for simplicity, CSS determines effect
                if (typeof window !== 'undefined' && window.innerWidth < 1024) { // Check window exists
                    state.isPanelOpen = !state.isPanelOpen;
                } else {
                    state.isPanelCollapsed = !state.isPanelCollapsed;
                }
            }),
             closeSidebar: () => set({ isPanelOpen: false }), // Primarily for mobile overlay
            openModelModal: () => set({ isModelModalOpen: true }),
            closeModelModal: () => set({ isModelModalOpen: false }),
            openServersModal: () => {
                set({ isServersModalOpen: true });
                (get() as StoreType).fetchServerConfig(); // Assert get() type
            },
            closeServersModal: () => set({ isServersModalOpen: false, selectedServerName: null }),
            openConfigEditor: (fileName: string) => set({ isConfigEditorOpen: true, currentEditingConfigFile: fileName }), // Type parameter
            closeConfigEditor: () => set({ isConfigEditorOpen: false, currentEditingConfigFile: null }),

            // --- Chat Actions ---
            setMessages: (messages: Message[]) => set({ messages: messages }), // Type parameter
            addMessage: (message: Message) => set((state: WritableDraft<StoreType>) => { // Use WritableDraft
                state.messages.push(message);
            }),
            setCurrentConversationId: (id: string | null) => set({ currentConversationId: id }), // Type parameter
            clearConversation: () => set({ messages: [], currentConversationId: null }), // Clear messages and ID

            // --- Conversation List Actions ---
            setConversations: (conversations: ConversationSummary[]) => set({ // Type parameter
                // Sort conversations by updatedAt descending when setting
                conversations: [...conversations].sort((a, b) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime())
            }),
            updateConversationInList: (conversation: ConversationSummary) => set((state: WritableDraft<StoreType>) => { // Use WritableDraft
                const index = state.conversations.findIndex((c: ConversationSummary) => c.id === conversation.id); // Type c
                if (index !== -1) {
                    state.conversations[index] = conversation;
                } else {
                    state.conversations.push(conversation);
                }
                // Re-sort after update/add
                state.conversations.sort((a: ConversationSummary, b: ConversationSummary) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime()); // Type a, b
            }),
            removeConversationFromList: (id: string) => set((state: WritableDraft<StoreType>) => { // Use WritableDraft
                state.conversations = state.conversations.filter((c: ConversationSummary) => c.id !== id); // Type c
            }),
            fetchConversationsList: async () => {
                try {
                    const convos = await fetchConversationsListApi();
                    set({ conversations: [...convos].sort((a: ConversationSummary, b: ConversationSummary) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime()) }); // Type a, b
                } catch (error) {
                    console.error("Failed to fetch conversations list:", error);
                    // Error toast handled by API client
                }
            },

            // --- Provider Actions ---
            setCurrentProvider: (provider: string) => set({ currentProvider: provider }), // Type parameter
            setProviders: (providers: Providers) => set({ providers: providers }), // Type parameter
            setProviderModels: (models: ProviderModels) => set({ providerModels: models }), // Type parameter
            fetchProviders: async () => {
                try {
                    const data = await fetchProvidersApi();
                    set({
                        providers: data.providers || {},
                        providerModels: data.models || {},
                        currentProvider: data.current || '',
                    });
                } catch (error) {
                    console.error("Failed to fetch providers:", error);
                    // Error toast handled by API client
                }
            },
            switchProviderAndModel: async (provider: string, model: string) => { // Type parameters
                // API call triggers backend change, socket event 'model-changed' updates state
                await switchProviderAndModelApi(provider, model);
                // Optimistic update could be done here, but socket is more reliable
            },
            updateApiKey: async (provider: string, apiKey: string) => { // Type parameters
                 // API call triggers backend change, potential socket event updates state if current provider changed
                 return await updateApiKeyApi(provider, apiKey);
            },

            // --- Server Actions ---
            setServersStatus: (servers: ServerInfo[]) => { // Type parameter (already correct)
                set({ servers });
                // Update derived text state
                let statusText = 'No servers connected';
                 if (Array.isArray(servers)) {
                     const connectedCount = servers.filter(s => s.status === 'connected').length;
                     const errorCount = servers.filter(s => s.status === 'error').length;

                     if (servers.length === 0) {
                         statusText = 'No servers configured';
                     } else if (connectedCount === 0 && errorCount === 0) {
                         statusText = 'Connecting...'; // Or 'Disconnected'
                     } else {
                         statusText = `${connectedCount} server${connectedCount !== 1 ? 's' : ''} connected`;
                         if (errorCount > 0) {
                             statusText += ` (${errorCount} error${errorCount !== 1 ? 's' : ''})`;
                         }
                     }
                 }
                 set({ connectedServersText: statusText });
            },
            setServerConfig: (config: McpServersConfig) => set({ serverConfig: config }), // Type parameter
            setSelectedServerName: (name: string | null) => set({ selectedServerName: name }), // Type parameter
            fetchServerConfig: async () => {
                try {
                    const config = await fetchServerConfigApi();
                    set({ serverConfig: config });
                } catch (error) {
                    console.error("Failed to fetch server config:", error);
                    set({ serverConfig: { mcpServers: {} } }); // Reset on error
                }
            },
            saveServerConfig: async (config: McpServersConfig) => { // Type parameter
                return await saveServerConfigurationsApi(config);
                // No state update here, requires backend restart
            },

            // --- Tool Actions ---
            setAllToolsData: (tools: ToolsByServer) => set({ allToolsData: tools }), // Type parameter

            // --- Socket Actions ---
            setSocket: (socket: Socket | null) => set({ _socket: socket }), // Type parameter
            emitUserMessage: (message: string) => { // Type parameter
                const socket = (get() as StoreType)._socket; // Assert get() type
                if (socket) {
                    socket.emit('user-message', { message });
                    // Optimistically add user message to state? Or wait for history update?
                    // get().addMessage({ role: 'human', content: message }); // Example optimistic update
                } else {
                    console.error("Socket not available to emit user message");
                }
            },
            emitClearConversation: () => (get() as StoreType)._socket?.emit('clear-conversation'), // Assert get() type
            emitNewConversation: () => (get() as StoreType)._socket?.emit('new-conversation'), // Assert get() type
            emitLoadConversation: (id: string) => (get() as StoreType)._socket?.emit('load-conversation', { id }), // Type parameter & assert get() type

            // --- Combined Actions ---
            fetchInitialData: async () => {
                (get() as StoreType).setStatusMessage('Loading initial data...'); // Assert get() type
                await Promise.all([
                    (get() as StoreType).fetchConversationsList(), // Assert get() type
                    (get() as StoreType).fetchProviders() // Assert get() type
                ]);
                // Determine initial conversation to load (if any)
                const { conversations, currentConversationId, emitLoadConversation } = get() as StoreType; // Assert get() type
                if (!currentConversationId && conversations.length > 0) {
                    // Assuming conversations are already sorted by update time
                    if (conversations[0]) {
                        console.log(`Loading most recent conversation: ${conversations[0].id}`);
                        emitLoadConversation(conversations[0].id);
                        // Status will be set to 'Ready' by the 'conversation-loaded' socket handler
                    } else {
                         (get() as StoreType).setStatusMessage('Ready'); // Assert get() type
                    }
                } else if (!currentConversationId) {
                     (get() as StoreType).setStatusMessage('Ready'); // No conversations exist // Assert get() type
                }
                // If currentId exists, assume socket 'conversation-loaded' handled it
            },

        })),
        { name: 'mcp-multi-client-store' } // Name for devtools
    )
);

