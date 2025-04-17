// Replicates functionality of web/js/api/apiClient.js using fetch or axios
import axios from 'axios'; // Using axios for simplicity, can use fetch too
import toast from 'react-hot-toast';
import { ConversationSummary, McpServersConfig, ProviderConfig, Providers, ProviderModels } from '@/store/store'; // Import types

const API_BASE = '/api'; // Matches Vite proxy

// --- Axios Instance (optional but good practice) ---
const apiClient = axios.create({
  baseURL: API_BASE,
  headers: {
    'Content-Type': 'application/json',
  },
});

// --- Error Handling ---
const handleError = (error: any, context: string): Error => {
  let message = 'An unknown error occurred';
  if (axios.isAxiosError(error)) {
    message = error.response?.data?.error || error.message;
  } else if (error instanceof Error) {
    message = error.message;
  }
  console.error(`API Error (${context}):`, message, error);
  toast.error(`API Error: ${message}`);
  return new Error(message); // Re-throw a generic error
};

// --- Conversation API Calls ---

export const fetchConversationsListApi = async (): Promise<ConversationSummary[]> => {
  try {
    const response = await apiClient.get<{ conversations: ConversationSummary[] }>('/conversations');
    return response.data.conversations || [];
  } catch (error) {
    handleError(error, 'fetchConversationsList');
    return []; // Return empty array on error
  }
};

export const renameConversationApi = async (conversationId: string, newTitle: string): Promise<void> => {
  try {
    await apiClient.post(`/conversations/${conversationId}/rename`, { title: newTitle });
    // Success toast handled by caller or socket event
  } catch (error) {
    throw handleError(error, 'renameConversation');
  }
};

export const deleteConversationApi = async (conversationId: string): Promise<void> => {
  try {
    await apiClient.delete(`/conversations/${conversationId}`);
     // Success toast handled by caller or socket event
  } catch (error) {
    throw handleError(error, 'deleteConversation');
  }
};

// --- AI/Model API Calls ---

interface ProvidersResponse {
    current: string;
    providers: Providers; // Should include apiKeySet flag from backend
    models: ProviderModels;
}

export const fetchProvidersApi = async (): Promise<ProvidersResponse> => {
  try {
    const response = await apiClient.get<ProvidersResponse>('/ai/providers');
    return response.data;
  } catch (error) {
    handleError(error, 'fetchProviders');
    return { current: '', providers: {}, models: {} }; // Default structure on error
  }
};

interface SwitchModelResponse {
    provider: string;
    model: string;
}
export const switchProviderAndModelApi = async (provider: string, model: string): Promise<SwitchModelResponse> => {
  try {
    const response = await apiClient.post<SwitchModelResponse>('/ai/model', { provider, model });
     // Success toast/UI update handled by socket event listener
    return response.data;
  } catch (error) {
    throw handleError(error, 'switchProviderAndModel');
  }
};

interface ApiKeyResponse {
    success: boolean;
    message?: string;
    // Include other potential fields like provider/model if returned
}
export const updateApiKeyApi = async (provider: string, apiKey: string): Promise<ApiKeyResponse> => {
  try {
    const response = await apiClient.post<ApiKeyResponse>('/ai/keys', { provider, apiKey });
    // Success toast handled by caller or socket event
    return response.data;
  } catch (error) {
    throw handleError(error, 'updateApiKey');
  }
};

// --- Server Config API Calls ---

export const fetchServerConfigApi = async (): Promise<McpServersConfig> => {
  try {
    const response = await apiClient.get<McpServersConfig>('/servers/config');
    return response.data || { mcpServers: {} };
  } catch (error) {
    handleError(error, 'fetchServerConfig');
    return { mcpServers: {} }; // Default structure on error
  }
};

interface SaveConfigResponse {
    success: boolean;
    message?: string;
    needsRestart?: boolean;
}
export const saveServerConfigurationsApi = async (config: McpServersConfig): Promise<SaveConfigResponse> => {
  try {
    const response = await apiClient.post<SaveConfigResponse>('/servers/config', { config });
    // Success toast handled by caller
    if (response.data.message) {
        toast.success(response.data.message);
    }
    if (response.data.needsRestart) {
        // Replace custom JSX toast with standard warning toast
        toast.warning('Restart the application to apply server changes.', {
             duration: 6000,
             id: 'server-restart-warning' // Optional ID to prevent duplicates
        });
    }
    return response.data;
  } catch (error) {
    throw handleError(error, 'saveServerConfigurations');
  }
};

// --- General Config File API Calls ---

interface FetchFileResponse {
    content: string;
}
export const fetchConfigFileApi = async (fileName: string): Promise<FetchFileResponse> => {
  try {
    const response = await apiClient.get<FetchFileResponse>(`/config/${encodeURIComponent(fileName)}`);
    return response.data;
  } catch (error) {
    throw handleError(error, `fetchConfigFile(${fileName})`);
  }
};

export const saveConfigFileApi = async (fileName: string, content: string): Promise<SaveConfigResponse> => {
  try {
    const response = await apiClient.post<SaveConfigResponse>(`/config/${encodeURIComponent(fileName)}`, { content });
    // Success toast handled by caller
     if (response.data.message) {
        toast.success(response.data.message);
    }
     if (response.data.needsRestart) {
        // Replace custom JSX toast with standard warning toast
        toast.warning('Restart the application for configuration changes to take effect.', {
             duration: 6000,
             id: 'config-restart-warning' // Optional ID to prevent duplicates
        });
    }
    return response.data;
  } catch (error) {
    throw handleError(error, `saveConfigFile(${fileName})`);
  }
};
