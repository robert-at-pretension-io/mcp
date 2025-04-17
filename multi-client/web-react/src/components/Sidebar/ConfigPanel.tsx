import React from 'react';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
// Removed unused faEdit
import { faCog, faRobot, faServer, faList } from '@fortawesome/free-solid-svg-icons';
import AccordionSection from './AccordionSection';
import { useStore } from '@/store/store';

const ConfigPanel: React.FC = () => {
    const { openConfigEditor } = useStore(state => ({
        openConfigEditor: state.openConfigEditor,
    }));

  const handleEditClick = (fileName: string) => {
    openConfigEditor(fileName);
  };

  const title = (
    <span className="flex items-center justify-between w-full">
      <span className="flex items-center gap-2">
        <FontAwesomeIcon icon={faCog} className="text-primary" /> Configuration
      </span>
      {/* Maybe remove the edit button from header if options are below? */}
      {/* <button
        onClick={(e) => { e.stopPropagation(); /* Maybe open first config? * / }}
        className="hover:bg-primary/10 p-1.5 rounded-full"
        title="Edit configuration files"
      >
        <FontAwesomeIcon icon={faEdit} className="text-primary" />
      </button> */}
    </span>
  );

  return (
    <AccordionSection title={title}>
      <div className="p-4">
        <div className="config-options grid grid-cols-2 gap-3">
          <button
            onClick={() => handleEditClick('ai_config.json')}
            className="config-option flex flex-col items-center justify-center p-3 bg-gray-50 dark:bg-gray-700 hover:bg-gray-100 dark:hover:bg-gray-600 rounded-lg transition-colors"
          >
            <FontAwesomeIcon icon={faRobot} className="text-xl text-primary mb-2" />
            <span className="text-sm">AI Config</span>
          </button>
          <button
            onClick={() => handleEditClick('servers.json')}
            className="config-option flex flex-col items-center justify-center p-3 bg-gray-50 dark:bg-gray-700 hover:bg-gray-100 dark:hover:bg-gray-600 rounded-lg transition-colors"
          >
            <FontAwesomeIcon icon={faServer} className="text-xl text-primary mb-2" />
            <span className="text-sm">Servers</span>
          </button>
          <button
            onClick={() => handleEditClick('provider_models.toml')}
            className="config-option flex flex-col items-center justify-center p-3 bg-gray-50 dark:bg-gray-700 hover:bg-gray-100 dark:hover:bg-gray-600 rounded-lg transition-colors"
          >
            <FontAwesomeIcon icon={faList} className="text-xl text-primary mb-2" />
            <span className="text-sm">Models</span>
          </button>
        </div>
      </div>
    </AccordionSection>
  );
};

export default ConfigPanel;
