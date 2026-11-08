import { ModBatchPanel } from "../../settings/panels/ModBatchPanel";
import { Button } from "../../../components/ui/Button";
import { ModCatalogManager } from "../../settings/panels/ModCatalogManager";
import { ModProcessingPanel } from "../../settings/panels/ModProcessingPanel";
import type { ModWorkflowController } from "./useModWorkflow";

export function ModWorkspace({ workflow }: { workflow: ModWorkflowController }) {
  const { view, draft, actions, en } = workflow;
  if (view === "batch") return <ModBatchPanel workflow={workflow} />;
  if (view === "processing" && draft) return <ModProcessingPanel workflow={workflow} />;
  return <>
    {draft && <div className="mod-draft-notice">
      <p>{en ? "You have an unfinished processing draft." : "有一份尚未完成的 Mod 加工草稿。"}</p>
      <Button size="sm" variant="ghost" onClick={actions.resume}>{en ? "Continue editing draft" : "继续编辑加工草稿"}</Button>
      <Button size="sm" variant="ghost" disabled={workflow.busy} onClick={actions.discardDraft}>{en ? "Discard draft" : "放弃草稿"}</Button>
    </div>}
    <ModCatalogManager catalog={workflow.catalog} accounts={workflow.accounts} language={en ? "en-US" : "zh-CN"}
      minimalMode={workflow.minimalMode} autoOpenAdd={workflow.openAdd} edition={workflow.libraryEdition}
      onEditionChange={actions.setLibraryEdition} onBatch={actions.openBatch}
      onProcess={async capsule => {
        const edition = capsule.edition === "Global" ? "Global" : "CN";
        actions.requestProcessing({ origin: "library", edition,
          source: { name: capsule.name, processed: capsule.processed || capsule.update_required } });
      }}
      onCreate={edition => actions.requestProcessing({ origin: "library", edition: edition === "Global" ? "Global" : "CN" })} />
  </>;
}
