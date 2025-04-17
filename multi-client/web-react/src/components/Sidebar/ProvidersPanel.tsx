import React from 'react';
// Removed duplicate React import
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faPlug, faCheckCircle } from '@fortawesome/free-solid-svg-icons';
import AccordionSection from './AccordionSection';
import { useStore, StoreType } from '@/store/store'; // Import StoreType
import { shallow } from 'zustand/shallow';
import { escapeHtml } from '@/utils/helpers';
import toast from 'react-hot-toast';

const ProvidersPanel: React.FC = () => {
  const { providers, currentProvider, providerModels, switchProviderAndModel } = useStore(
    (state: StoreType) => ({ // Type state
      providers: state.providers,
      currentProvider: state.currentProvider,
      providerModels: state.providerModels,
      switchProviderAndModel: state.switchProviderAndModel, // Get action from store
    }),
    shallow // Correct usage: Pass shallow as the second argument
  );

  const handleSelectProvider = async (name: string) => {
    if (!name || name === currentProvider) return;

    // Find the default/first model for this provider to switch
    const providerConfig = providers[name];
    const modelsForProvider = providerModels[name.toLowerCase()]?.models || [];
    const targetModel = providerConfig?.model || modelsForProvider[0] || ''; // Use configured or first suggested

    if (!targetModel) {
      toast.error(`No model configured or suggested for provider ${name}. Cannot switch.`);
      return;
    }

    toast.promise(
        switchProviderAndModel(name, targetModel),
        {
          loading: `Switching to ${name}...`,
          success: `Switched to ${name}`, // Success message handled by socket event now
          error: (err) => `Failed to switch: ${err.message || 'Unknown error'}`,
        }
    );
    // Actual state update will come through socket 'model-changed' event handled in useSocket hook
  };

  const title = (
    <span className="flex items-center gap-2">
      <FontAwesomeIcon icon={faPlug} className="text-primary" /> AI Providers
    </span>
  );

  const sortedProviderNames = Object.keys(providers).sort();

  return (
    <AccordionSection title={title}>
      <div className="space-y-1">
        {sortedProviderNames.length === 0 ? (
          <div className="text-sm text-gray-500 p-3 text-center">No providers configured</div>
        ) : (
          sortedProviderNames.map((name) => {
            const config = providers[name];
            const isActive = name === currentProvider;
            const activeClasses = isActive ? 'bg-primary/10 dark:bg-primary/20 border-primary' : 'border-transparent hover:bg-gray-100 dark:hover:bg-gray-700';

            return (
              <div
                key={name}
                onClick={() => handleSelectProvider(name)}
                className={`provider-item p-2 rounded-lg border ${activeClasses} cursor-pointer transition-colors text-sm`}
              >
                <div className="flex justify-between items-center mb-1">
                  <span className="font-medium">{escapeHtml(name)}</span>
                  {isActive && <FontAwesomeIcon icon={faCheckCircle} className="text-success" title="Active Provider" />}
                </div>
                <div className="text-xs text-gray-500 dark:text-gray-400 truncate" title={escapeHtml(config.model || '')}>
                  Model: {escapeHtml(config.model || 'Default')}
                </div>
              </div>
            );
          })
        )}
      </div>
    </AccordionSection>
  );
};

export default ProvidersPanel;
