import React from 'react'; // Add React back if needed for useMemo
import { useMemo } from 'react'; // Import useMemo
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faColumns, faServer, faRobot, faCog } from '@fortawesome/free-solid-svg-icons';
import { useStore, StoreType } from '@/store/store'; // Import StoreType
import { shallow } from 'zustand/shallow'; // Import shallow

const Header: React.FC = () => {
  // Select multiple properties - use shallow
  const {
    connectedServersText,
    currentProvider,
    currentModel,
    toggleSidebar,
    connectedServersText,
    currentProvider,
    providers, // Select base state needed for derivation
    toggleSidebar,
    openModelModal,
    openServersModal,
  } = useStore(
    (state: StoreType) => ({ // Type state
      connectedServersText: state.connectedServersText,
      currentProvider: state.currentProvider,
      providers: state.providers, // Select the providers object
      toggleSidebar: state.toggleSidebar,
      openModelModal: state.openModelModal,
      openServersModal: state.openServersModal,
    }),
    shallow // Use shallow since we select an object
  );

  // Derive currentModel outside the selector using useMemo
  const currentModel = useMemo(() => {
      return providers[currentProvider]?.model || 'N/A';
  }, [providers, currentProvider]);

  const modelDisplay = useMemo(() => (
      currentModel !== 'N/A' && currentProvider
          ? `${currentModel} (${currentProvider})`
          : 'Loading...'
  ), [currentModel, currentProvider]);

  return (
    <header className="header-gradient text-white py-4 px-6 shadow-md sticky top-0 z-40">
      <div className="container mx-auto flex flex-col md:flex-row items-center justify-between gap-4">
        <div className="flex items-center">
          <h1 className="text-2xl font-bold">MCP Multi-Client</h1>
        </div>
        <div className="info-controls flex items-center gap-4">
          <button
            onClick={toggleSidebar}
            className="bg-white/10 hover:bg-white/20 p-2 rounded-lg transition-colors"
            title="Toggle Sidebar"
          >
            <FontAwesomeIcon icon={faColumns} />
          </button>
          <div className="server-info flex flex-col md:flex-row gap-4 bg-white/10 rounded-lg px-4 py-2">
            <div className="info-item flex items-center gap-2">
              <FontAwesomeIcon icon={faServer} className="text-white/80" />
              <span id="connected-servers" className="text-sm font-medium">
                {connectedServersText}
              </span>
              <button
                id="manage-servers-btn"
                onClick={openServersModal}
                className="hover:bg-white/10 p-1 rounded"
                title="Manage servers"
              >
                <FontAwesomeIcon icon={faCog} />
              </button>
            </div>
            <div className="info-item model-info flex items-center gap-2">
              <FontAwesomeIcon icon={faRobot} className="text-white/80" />
              <span id="ai-model" className="text-sm font-medium" title={`Provider: ${currentProvider}, Model: ${currentModel}`}>
                {modelDisplay}
              </span>
              <button
                id="change-model-btn"
                onClick={openModelModal}
                className="hover:bg-white/10 p-1 rounded"
                title="Change AI model"
              >
                <FontAwesomeIcon icon={faCog} />
              </button>
            </div>
          </div>
        </div>
      </div>
    </header>
  );
};

export default Header;
