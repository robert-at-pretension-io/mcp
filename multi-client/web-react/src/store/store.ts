import { create } from 'zustand';
import { immer } from 'zustand/middleware/immer';
import { devtools } from 'zustand/middleware';
import {
    fetchConversationsListApi,
    fetchProvidersApi,
    switchProviderAndModelApi,
    updateApiKeyApi,
    fetchServerConfigApi,
    saveServerConfigurationsApi,
    fetchConfigFileApi,
    saveConfigFileApi,
    // Import other API functions as needed
} from '@/services/api'; // Assuming API functions are defined
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
type StoreType = AppState & AppActions & {
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
        immer((set, get) => ({
            ...initialState,
            _socket: null, // Initialize internal socket state

            // --- UI Actions ---
            setThinking: (thinking, message = 'AI is thinking...') => set((state) => {
                state.isThinking = thinking;
                if (thinking) {
                    state.thinkingMessage = message;
                    state.statusMessage = message; // Also update main status
                } else {
                    state.statusMessage = 'Ready'; // Reset status when not thinking
                }
            }),
            setStatusMessage: (message) => set({ statusMessage: message }),
            toggleSidebar: () => set((state) => {
                // Logic depends on screen size, ideally handled by CSS media queries reacting to classes
                // This toggle might control both states for simplicity, CSS determines effect
                if (window.innerWidth < 1024) { // Example breakpoint for mobile
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
                get().fetchServerConfig(); // Fetch config when opening
            },
            closeServersModal: () => set({ isServersModalOpen: false, selectedServerName: null }),
            openConfigEditor: (fileName) => set({ isConfigEditorOpen: true, currentEditingConfigFile: fileName }),
            closeConfigEditor: () => set({ isConfigEditorOpen: false, currentEditingConfigFile: null }),

            // --- Chat Actions ---
            setMessages: (messages) => set({ messages: messages }),
            addMessage: (message) => set((state) => {
                state.messages.push(message);
            }),
            setCurrentConversationId: (id) => set({ currentConversationId: id }),
            clearConversation: () => set({ messages: [], currentConversationId: null }), // Clear messages and ID

            // --- Conversation List Actions ---
            setConversations: (conversations) => set({
                // Sort conversations by updatedAt descending when setting
                conversations: [...conversations].sort((a, b) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime())
            }),
            updateConversationInList: (conversation) => set((state) => {
                const index = state.conversations.findIndex(c => c.id === conversation.id);
                if (index !== -1) {
                    state.conversations[index] = conversation;
                } else {
                    state.conversations.push(conversation);
                }
                // Re-sort after update/add
                state.conversations.sort((a, b) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime());
            }),
            removeConversationFromList: (id) => set((state) => {
                state.conversations = state.conversations.filter(c => c.id !== id);
            }),
            fetchConversationsList: async () => {
                try {
                    const convos = await fetchConversationsListApi();
                    set({ conversations: [...convos].sort((a, b) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime()) });
                } catch (error) {
                    console.error("Failed to fetch conversations list:", error);
                    // Error toast handled by API client
                }
            },

            // --- Provider Actions ---
            setCurrentProvider: (provider) => set({ currentProvider: provider }),
            setProviders: (providers) => set({ providers: providers }),
            setProviderModels: (models) => set({ providerModels: models }),
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
            switchProviderAndModel: async (provider, model) => {
                // API call triggers backend change, socket event 'model-changed' updates state
                await switchProviderAndModelApi(provider, model);
                // Optimistic update could be done here, but socket is more reliable
            },
            updateApiKey: async (provider, apiKey) => {
                 // API call triggers backend change, potential socket event updates state if current provider changed
                 return await updateApiKeyApi(provider, apiKey);
            },

            // --- Server Actions ---
            setServersStatus: (servers) => {
                 set({ servers: servers });
                 // Update derived text state
                 let count = 0;
                 let statusText = 'No servers connected';
                 if (Array.isArray(servers)) {
                     const connectedCount = servers.filter(s => s.status === 'connected').length;
                     const errorCount = servers.filter(s => s.status === 'error').length;
                     count = connectedCount;

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
            setServerConfig: (config) => set({ serverConfig: config }),
            setSelectedServerName: (name) => set({ selectedServerName: name }),
            fetchServerConfig: async () => {
                try {
                    const config = await fetchServerConfigApi();
                    set({ serverConfig: config });
                } catch (error) {
                    console.error("Failed to fetch server config:", error);
                    set({ serverConfig: { mcpServers: {} } }); // Reset on error
                }
            },
            saveServerConfig: async (config) => {
                return await saveServerConfigurationsApi(config);
                // No state update here, requires backend restart
            },

            // --- Tool Actions ---
            setAllToolsData: (tools) => set({ allToolsData: tools }),

            // --- Socket Actions ---
            setSocket: (socket) => set({ _socket: socket }),
            emitUserMessage: (message) => {
                const socket = get()._socket;
                if (socket) {
                    socket.emit('user-message', { message });
                    // Optimistically add user message to state? Or wait for history update?
                    // get().addMessage({ role: 'human', content: message }); // Example optimistic update
                } else {
                    console.error("Socket not available to emit user message");
                }
            },
            emitClearConversation: () => get()._socket?.emit('clear-conversation'),
            emitNewConversation: () => get()._socket?.emit('new-conversation'),
            emitLoadConversation: (id) => get()._socket?.emit('load-conversation', { id }),

            // --- Combined Actions ---
            fetchInitialData: async () => {
                get().setStatusMessage('Loading initial data...');
                await Promise.all([
                    get().fetchConversationsList(),
                    get().fetchProviders()
                ]);
                // Determine initial conversation to load (if any)
                const { conversations, currentConversationId, emitLoadConversation } = get();
                if (!currentConversationId && conversations.length > 0) {
                    // Assuming conversations are already sorted by update time
                    if (conversations[0]) {
                        console.log(`Loading most recent conversation: ${conversations[0].id}`);
                        emitLoadConversation(conversations[0].id);
                        // Status will be set to 'Ready' by the 'conversation-loaded' socket handler
                    } else {
                         get().setStatusMessage('Ready');
                    }
                } else if (!currentConversationId) {
                     get().setStatusMessage('Ready'); // No conversations exist
                }
                // If currentId exists, assume socket 'conversation-loaded' handled it
            },

        })),
        { name: 'mcp-multi-client-store' } // Name for devtools
    )
);

// Add computed property for connectedServersText outside the create call
useStore.subscribe((state) => 
    (state.servers, // Select the servers state
    (servers) => { // Listener function
        let statusText = 'No servers connected';
        if (Array.isArray(servers)) {
            const connectedCount = servers.filter(s => s.status === 'connected').length;
            const errorCount = servers.filter(s => s.status === 'error').length;

            if (servers.length === 0) {
                statusText = 'No servers configured';
            } else if (connectedCount === 0 && errorCount === 0 && servers.some(s => s.status === 'connecting')) {
                 statusText = 'Connecting...';
            } else if (connectedCount === 0 && errorCount === 0) {
                 statusText = 'Disconnected';
            }
            else {
                statusText = `${connectedCount} server${connectedCount !== 1 ? 's' : ''} connected`;
                if (errorCount > 0) {
                    statusText += ` (${errorCount} error${errorCount !== 1 ? 's' : ''})`;
                }
            }
        }
        useStore.setState({ connectedServersText: statusText });
    })
);
