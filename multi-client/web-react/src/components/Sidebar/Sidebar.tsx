import React from 'react';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faTimes } from '@fortawesome/free-solid-svg-icons';
import ConversationsPanel from './ConversationsPanel';
import ProvidersPanel from './ProvidersPanel';
import ToolsPanel from './ToolsPanel';
import ConfigPanel from './ConfigPanel';
import { useStore, StoreType } from '@/store/store'; // Import StoreType
import { shallow } from 'zustand/shallow'; // Import shallow

const Sidebar: React.FC = () => {
    // Select multiple properties - use shallow
    const { isPanelOpen, closeSidebar } = useStore(
        (state: StoreType) => ({ // Type state
            isPanelOpen: state.isPanelOpen,
            closeSidebar: state.closeSidebar,
        }),
        shallow
    );

    // Classes to control visibility and transition based on state
    const sidebarClasses = `
        right-panel w-80 space-y-6 absolute lg:static top-0 right-0 h-full
        bg-gray-50 dark:bg-gray-900 p-4 lg:p-0
        lg:bg-transparent lg:dark:bg-transparent
        transform transition-transform duration-300 ease-in-out z-30 lg:z-auto
        shadow-lg lg:shadow-none overflow-y-auto
        ${isPanelOpen ? 'translate-x-0' : 'translate-x-full'} lg:translate-x-0
    `;


  return (
    <div id="right-panel" className={sidebarClasses}>
      {/* Close button for mobile */}
      <button
        id="close-right-panel-btn"
        onClick={closeSidebar}
        className="absolute top-2 right-2 lg:hidden btn-icon text-gray-500 dark:text-gray-400"
        title="Close Sidebar"
      >
        <FontAwesomeIcon icon={faTimes} />
      </button>

      {/* Panels */}
      <ConversationsPanel />
      <ProvidersPanel />
      <ToolsPanel />
      <ConfigPanel />
    </div>
  );
};

export default Sidebar;
