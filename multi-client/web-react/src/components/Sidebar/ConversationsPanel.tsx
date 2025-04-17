import React from 'react';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faComments, faPlus, faEdit, faTrash } from '@fortawesome/free-solid-svg-icons';
import AccordionSection from './AccordionSection';
import { useStore, ConversationSummary } from '@/store/store';
import { formatRelativeTime, escapeHtml } from '@/utils/helpers';
import toast from 'react-hot-toast';
import { renameConversationApi, deleteConversationApi } from '@/services/api'; // Assuming API functions exist

const ConversationsPanel: React.FC = () => {
  const {
    conversations,
    currentConversationId,
    emitLoadConversation,
    emitNewConversation,
    updateConversationInList, // Add this from store
    removeConversationFromList, // Add this from store
  } = useStore((state: any) => ({
    conversations: state.conversations,
    currentConversationId: state.currentConversationId,
    emitLoadConversation: state.emitLoadConversation,
    emitNewConversation: state.emitNewConversation,
    updateConversationInList: state.updateConversationInList,
    removeConversationFromList: state.removeConversationFromList,
  }));

  const handleSelect = (id: string) => {
    if (id !== currentConversationId) {
      emitLoadConversation(id);
    }
  };

  const handleRename = async (e: React.MouseEvent, id: string, currentTitle: string) => {
      e.stopPropagation(); // Prevent selection
      const newTitle = prompt('Enter new title:', currentTitle || '');
      if (newTitle !== null && newTitle.trim() !== (currentTitle || '')) {
          try {
              await renameConversationApi(id, newTitle.trim());
              // Optimistic update (or wait for socket event if backend sends one)
              // Ensure conversations is treated as ConversationSummary[]
              const updatedConvo = (conversations as ConversationSummary[]).find(c => c.id === id);
              if (updatedConvo) {
                  updateConversationInList({ ...updatedConvo, title: newTitle.trim(), updatedAt: new Date().toISOString() });
              }
              toast.success('Conversation renamed');
          } catch (error: any) {
              toast.error(`Failed to rename: ${error.message}`);
          }
      }
  };

  const handleDelete = async (e: React.MouseEvent, id: string) => {
      e.stopPropagation(); // Prevent selection
      if (window.confirm('Are you sure you want to delete this conversation?')) {
          try {
              await deleteConversationApi(id);
              const wasCurrent = currentConversationId === id;
              // Optimistic update (or wait for socket event)
              removeConversationFromList(id);
              toast.success('Conversation deleted');
              if (wasCurrent) {
                  emitNewConversation(); // Load a new blank conversation
              }
          } catch (error: any) {
              toast.error(`Failed to delete: ${error.message}`);
          }
      }
  };


  const title = (
    <span className="flex items-center justify-between w-full">
      <span className="flex items-center gap-2">
        <FontAwesomeIcon icon={faComments} className="text-primary" /> Conversations
      </span>
      <button
        onClick={(e) => { e.stopPropagation(); emitNewConversation(); }}
        className="hover:bg-primary/10 p-1.5 rounded-full"
        title="New conversation"
      >
        <FontAwesomeIcon icon={faPlus} className="text-primary" />
      </button>
    </span>
  );

  return (
    <AccordionSection title={title} startOpen={true}>
      <div className="max-h-60 overflow-y-auto space-y-1 pr-1"> {/* Added padding-right */}
        {(conversations as ConversationSummary[]).length === 0 ? ( // Cast conversations
          <div className="text-sm text-gray-500 p-3 text-center">No conversations</div>
        ) : (
          (conversations as ConversationSummary[]).map((c) => { // Cast conversations
            const isActive = c.id === currentConversationId;
            const activeClasses = isActive ? 'bg-primary/10 dark:bg-primary/20 border-primary' : 'border-transparent hover:bg-gray-100 dark:hover:bg-gray-700';
            let relativeTime = 'unknown time';
            let fullDate = 'Date unavailable';
            try {
                const date = new Date(c.updatedAt);
                if (!isNaN(date.getTime())) {
                    relativeTime = formatRelativeTime(date);
                    fullDate = date.toLocaleString();
                }
            } catch (e) { /* ignore */ }

            return (
              <div
                key={c.id}
                onClick={() => handleSelect(c.id)}
                className={`conversation-item p-2 rounded-lg border ${activeClasses} cursor-pointer transition-colors group relative text-sm`}
              >
                <div className="font-medium truncate mb-1">{escapeHtml(c.title || 'Untitled Conversation')}</div>
                <div className="text-xs text-gray-500 dark:text-gray-400 flex justify-between items-center">
                  <span className="truncate" title={`${escapeHtml(c.provider || '')} - ${escapeHtml(c.modelName || '')}`}>
                    {escapeHtml(c.provider?.substring(0, 8) || 'N/A')} - {escapeHtml(c.modelName?.substring(0, 12) || 'N/A')}
                  </span>
                  <span className="conversation-date flex-shrink-0 ml-2" title={fullDate}>{relativeTime}</span>
                </div>
                 <div className="conversation-actions absolute top-1 right-1 opacity-0 group-hover:opacity-100 transition-opacity flex gap-1">
                    <button onClick={(e) => handleRename(e, c.id, c.title)} className="btn-icon p-1 text-xs hover:bg-gray-200 dark:hover:bg-gray-600 rounded" title="Rename"><FontAwesomeIcon icon={faEdit} /></button>
                    <button onClick={(e) => handleDelete(e, c.id)} className="btn-icon p-1 text-xs text-danger hover:bg-danger/10 rounded" title="Delete"><FontAwesomeIcon icon={faTrash} /></button>
                </div>
              </div>
            );
          })
        )}
      </div>
    </AccordionSection>
  );
};

export default ConversationsPanel;
