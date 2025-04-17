import React, { useState, useRef, useEffect } from 'react';
import { useStore } from '@/store/store';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faPaperPlane, faTrash } from '@fortawesome/free-solid-svg-icons';
import Spinner from '@/components/common/Spinner'; // Assuming Spinner component exists

const ChatInput: React.FC = () => {
  const [inputText, setInputText] = useState('');
  const { isThinking, thinkingMessage, emitUserMessage, emitClearConversation } = useStore(
    (state) => ({
      isThinking: state.isThinking,
      thinkingMessage: state.thinkingMessage,
      emitUserMessage: state.emitUserMessage,
      emitClearConversation: state.emitClearConversation,
    })
  );
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const handleInputChange = (event: React.ChangeEvent<HTMLTextAreaElement>) => {
    setInputText(event.target.value);
    adjustTextareaHeight();
  };

  const adjustTextareaHeight = () => {
    const textarea = textareaRef.current;
    if (textarea) {
      textarea.style.height = 'auto'; // Reset height
      const scrollHeight = textarea.scrollHeight;
      // Set height based on content, capped at 300px, min 40px (adjust as needed)
      textarea.style.height = `${Math.max(Math.min(scrollHeight, 300), 40)}px`;
    }
  };

  // Adjust height on initial render and when text changes
  useEffect(() => {
    adjustTextareaHeight();
  }, [inputText]);

  const handleSubmit = (event?: React.FormEvent<HTMLFormElement>) => {
    event?.preventDefault();
    const message = inputText.trim();
    if (message && !isThinking) {
      emitUserMessage(message);
      setInputText(''); // Clear input after sending
      // Textarea height will auto-adjust due to useEffect dependency on inputText
    }
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      handleSubmit();
    }
  };

  const handleClear = () => {
      // Optional: Add confirmation dialog
      // if (window.confirm('Are you sure you want to clear the conversation?')) {
          emitClearConversation();
      // }
  };

  return (
    <form
      id="chat-form"
      onSubmit={handleSubmit}
      className="input-area bg-white dark:bg-gray-800 rounded-xl shadow-lg p-5 relative"
    >
      {isThinking && <Spinner message={thinkingMessage} />}
      <textarea
        ref={textareaRef}
        id="user-input"
        value={inputText}
        onChange={handleInputChange}
        onKeyDown={handleKeyDown}
        disabled={isThinking}
        className="w-full p-4 border border-gray-200 dark:border-gray-700 rounded-lg focus:ring-2 focus:ring-primary focus:border-primary resize-none dark:bg-gray-700 dark:text-white overflow-y-auto" // Added overflow-y-auto
        placeholder="Type your message here..."
        rows={1} // Start with one row, height adjusts automatically
      />
      <div className="button-group flex justify-between mt-4">
        <button
          id="send-button"
          type="submit"
          disabled={isThinking || !inputText.trim()}
          className="btn btn-primary py-2.5 px-5 rounded-lg font-medium flex items-center gap-2 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          <FontAwesomeIcon icon={faPaperPlane} />
          <span>{isThinking ? 'Sending…' : 'Send'}</span>
        </button>
        <button
          id="clear-button"
          onClick={handleClear}
          className="btn btn-secondary py-2.5 px-5 rounded-lg font-medium flex items-center gap-2"
          type="button"
          disabled={isThinking}
        >
          <FontAwesomeIcon icon={faTrash} /> Clear
        </button>
      </div>
    </form>
  );
};

export default ChatInput;