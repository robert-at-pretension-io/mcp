import React, { useState } from 'react';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { faTools } from '@fortawesome/free-solid-svg-icons';
import AccordionSection from './AccordionSection';
import { useStore } from '@/store/store';
import { shallow } from 'zustand/shallow';
import { escapeHtml } from '@/utils/helpers';
import { ToolsByServer, ToolInfo } from '@/store/store'; // Import types

import { StoreType } from '@/store/store'; // Import StoreType

const ToolsPanel: React.FC = () => {
  const [filterText, setFilterText] = useState('');
  // Correct usage: Pass shallow as the second argument
  const toolsByServer = useStore((state: StoreType) => state.allToolsData, shallow); // Type state

  const handleFilterChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    setFilterText(e.target.value);
  };

  const filteredTools = React.useMemo(() => {
    const term = filterText.toLowerCase().trim();
    // Cast toolsByServer to the correct type
    const typedToolsByServer = toolsByServer as ToolsByServer;
    if (!term && Object.keys(typedToolsByServer).length === 0) return {}; // No servers connected

    const out: { [server: string]: ToolInfo[] } = {}; // Use ToolInfo type
    let foundAny = false;

    for (const [srv, list] of Object.entries(typedToolsByServer)) {
      const filtered: ToolInfo[] = term // Ensure list is ToolInfo[]
        ? list.filter(
            (t) =>
              t.name.toLowerCase().includes(term) ||
              (t.description && t.description.toLowerCase().includes(term))
          )
        : list;

      if (filtered.length > 0) {
        // Sort tools alphabetically within each server
        out[srv] = filtered.sort((a, b) => a.name.localeCompare(b.name));
        foundAny = true;
      }
    }
    // Return null if no tools match the filter but servers exist
    return foundAny ? out : (Object.keys(typedToolsByServer).length > 0 ? null : {});
  }, [toolsByServer, filterText]);

  const title = (
    <span className="flex items-center gap-2">
      <FontAwesomeIcon icon={faTools} className="text-primary" /> Available Tools
    </span>
  );

  const emptyListClasses = "text-center text-sm text-gray-500 dark:text-gray-400 py-4";

  return (
    <AccordionSection title={title}>
      <div className="p-0"> {/* Remove padding from AccordionSection content */}
         <div className="px-4 pt-2 pb-1"> {/* Add padding here instead */}
            <input
              type="text"
              value={filterText}
              onChange={handleFilterChange}
              placeholder="Filter tools..."
              className="w-full mb-3 p-2 border border-gray-200 dark:border-gray-700 rounded-lg dark:bg-gray-700 dark:text-white text-sm"
            />
         </div>
        <div className="tools-list max-h-60 overflow-y-auto space-y-4 text-sm px-4 pb-2"> {/* Add padding here */}
          {filteredTools === null ? (
             <div className={emptyListClasses}>No tools match filter "{escapeHtml(filterText)}".</div>
          ) : Object.keys(filteredTools).length === 0 ? (
             <div className={emptyListClasses}>No tools available from connected servers.</div>
          ) : (
            Object.entries(filteredTools).map(([srv, tools]) => (
              <div key={srv} className="tool-server-group">
                <h4 className="text-xs font-semibold mb-2 text-gray-600 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700 pb-1 uppercase">
                  {escapeHtml(srv)}
                </h4>
                <div className="space-y-2">
                  {tools.map((t) => (
                    <div key={t.name} className="tool-item p-2 rounded bg-gray-50 dark:bg-gray-700/50">
                      <h5 className="text-sm font-medium">{escapeHtml(t.name)}</h5>
                      <p className="text-xs text-gray-600 dark:text-gray-400">
                        {escapeHtml(t.description || 'No description')}
                      </p>
                    </div>
                  ))}
                </div>
              </div>
            ))
          )}
        </div>
      </div>
    </AccordionSection>
  );
};

export default ToolsPanel;
