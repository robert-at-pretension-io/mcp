import React, { useEffect, useRef } from 'react';
import { useStore } from '@/store/store';
import MessageItem from './MessageItem';
import { shallow } from 'zustand/shallow';

const MessageList: React.FC = () => {
  const messages = useStore((state) => state.messages, shallow);
  const listRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom
  useEffect(() => {
    if (listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
      // Optional: More robust scrolling ensuring last item is fully visible
      // listRef.current.lastElementChild?.scrollIntoView({ behavior: 'smooth', block: 'end' });
    }
  }, [messages]); // Trigger scroll when messages change

  return (
    <div
      ref={listRef}
      id="conversation"
      className="flex-grow rounded-xl shadow-lg bg-white dark:bg-gray-800 p-6 overflow-y-auto max-h-[calc(100vh-240px)]" // Adjust max-h as needed
    >
      {messages.length === 0 ? (
        <div className="flex items-center justify-center h-full text-gray-400 dark:text-gray-500">
          <p className="text-center">Start a new conversation or select an existing one</p>
        </div>
      ) : (
        <div className="space-y-4">
          {messages.map((msg, idx) => (
            // Use a more stable key if messages have unique IDs from backend
            <MessageItem key={msg.id || `${msg.role}-${idx}`} message={msg} />
          ))}
        </div>
      )}
    </div>
  );
};

export default MessageList;