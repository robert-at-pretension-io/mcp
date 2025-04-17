import { useEffect, useRef } from 'react';
import { io, Socket } from 'socket.io-client';
import { useStore, Message, ServerInfo, ConversationSummary, ToolInfo, ToolsByServer } from '@/store/store'; // Import types and store hook
import toast from 'react-hot-toast';
import { shallow } from 'zustand/shallow';

// Define types for socket event data payloads based on backend/old frontend
interface ServersInfoData {
    servers: ServerInfo[];
}
interface HistoryData {
    history: Message[];
}
interface ThinkingData {
    status: boolean;
    message?: string;
}
interface ToolsInfoData extends ToolsByServer {} // Data is the object itself

interface ErrorData {
    message: string;
}
interface ModelChangedData {
    provider: string;
    model: string;
}
interface ConversationsListData {
    conversations: ConversationSummary[];
}
interface ConversationLoadedData extends ConversationSummary {
    messages: Message[];
}
interface ConversationSavedData extends ConversationSummary {}


export const useSocket = () => {
  const socketRef = useRef<Socket | null>(null);
  // Get only the setter functions needed from the store to avoid unnecessary re-renders
  const {
    setSocket,
    setStatusMessage,
    setServersStatus,
    setMessages,
    setThinking,
    setAllToolsData,
    addMessage, // For errors
    clearConversation,
    setCurrentProvider, // Needed for model-changed
    fetchProviders, // Needed for model-changed
    setConversations,
    setCurrentConversationId,
    updateConversationInList,
  } = useStore(
    (state) => ({
      setSocket: state.setSocket,
      setStatusMessage: state.setStatusMessage,
      setServersStatus: state.setServersStatus,
      setMessages: state.setMessages,
      setThinking: state.setThinking,
      setAllToolsData: state.setAllToolsData,
      addMessage: state.addMessage,
      clearConversation: state.clearConversation,
      setCurrentProvider: state.setCurrentProvider,
      fetchProviders: state.fetchProviders,
      setConversations: state.setConversations,
      setCurrentConversationId: state.setCurrentConversationId,
      updateConversationInList: state.updateConversationInList,
    }),
    shallow // Use shallow comparison
  );

  useEffect(() => {
    // Prevent multiple connections
    if (socketRef.current) return;

    console.log("Initializing Socket.IO connection...");
    // Connect to the server (Vite proxy handles redirection)
    // Use path option to ensure connection to the correct namespace if needed,
    // but usually not required if backend serves socket on default path '/'
    const socketInstance = io({
        // path: '/socket.io', // Usually not needed with standard setup + proxy
        transports: ['websocket'], // Optional: force websocket transport
    });
    socketRef.current = socketInstance;
    setSocket(socketInstance); // Save socket instance to store

    // --- Connection Events ---
    socketInstance.on('connect', () => {
      console.log('Socket connected:', socketInstance.id);
      setStatusMessage('Connected');
      // Request initial data? Backend might send automatically on connect.
    });

    socketInstance.on('disconnect', (reason) => {
      console.log('Socket disconnected:', reason);
      setStatusMessage(`Disconnected: ${reason}`);
      toast.error(`Disconnected: ${reason}`, { id: 'disconnect-toast' });
      // Reset relevant state on disconnect?
      setServersStatus([]);
      setAllToolsData({});
    });

    socketInstance.on('connect_error', (error) => {
      console.error('Socket connection error:', error);
      setStatusMessage(`Connection Error: ${error.message}`);
      toast.error(`Connection Error: ${error.message}`, { id: 'connect-error-toast' });
    });

    // --- Application Specific Event Handlers ---
    socketInstance.on('servers-info', (data: ServersInfoData) => {
      // console.log('Received servers-info:', data);
      setServersStatus(data.servers || []);
    });

    socketInstance.on('history', (data: HistoryData) => {
      // console.log('Received history:', data);
      setMessages(data.history || []);
    });

    socketInstance.on('history-update', (data: HistoryData) => {
      // console.log('Received history-update:', data);
      setMessages(data.history || []);
    });

    socketInstance.on('thinking', (data: ThinkingData) => {
      // console.log('Received thinking:', data);
      setThinking(data.status, data.message);
    });

    socketInstance.on('tools-info', (data: ToolsInfoData) => {
       // console.log('Received tools-info:', data);
       setAllToolsData(data || {});
    });

    socketInstance.on('error', (data: ErrorData) => {
      console.error('Received error event:', data.message);
      setStatusMessage('Error');
      // Add error message to chat display
      addMessage({ role: 'error', content: data.message });
      toast.error(`Server Error: ${data.message}`);
    });

    socketInstance.on('conversation-cleared', () => {
      // console.log('Received conversation-cleared');
      clearConversation(); // Clear messages and current ID in store
      toast.success('Conversation Cleared');
    });

    socketInstance.on('model-changed', (data: ModelChangedData) => {
        // console.log('Received model-changed:', data);
        // Update state first (optimistic could be done here, but fetch ensures consistency)
        setCurrentProvider(data.provider);
        // Fetch providers again to get potentially updated model in config
        fetchProviders(); // This will update providers, models, and currentProvider in the store
        toast.success(`Switched to ${data.model} (${data.provider})`);
        // Close modal if open (handled in App.tsx or Modal component based on state change)
    });

    socketInstance.on('conversations-list', (data: ConversationsListData) => {
        // console.log('Received conversations-list:', data);
        setConversations(data.conversations || []);
    });

    socketInstance.on('conversation-loaded', (data: ConversationLoadedData) => {
        // console.log('Received conversation-loaded:', data);
        setCurrentConversationId(data.id);
        setMessages(data.messages || []);
        setStatusMessage('Ready'); // Set status to ready after loading
    });

     socketInstance.on('conversation-saved', (data: ConversationSavedData) => {
        // console.log('Received conversation-saved:', data);
        // Update state and UI list
        updateConversationInList(data);
        setCurrentConversationId(data.id); // Ensure current ID is set
        // Toast handled by the action initiator (e.g., after sending message)
    });


    // Cleanup on unmount
    return () => {
      console.log("Disconnecting socket...");
      socketInstance.disconnect();
      socketRef.current = null;
      setSocket(null);
    };
  }, [ // Dependencies for useEffect
      setSocket, setStatusMessage, setServersStatus, setMessages, setThinking,
      setAllToolsData, addMessage, clearConversation, setCurrentProvider,
      fetchProviders, setConversations, setCurrentConversationId, updateConversationInList
  ]); // Add all setters used inside useEffect

  // The hook doesn't need to return anything if actions are performed via store
};