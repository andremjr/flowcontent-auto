import { useState } from "react";
import type { AssemblyAiStatus, FlowBridgeStatus, ProjectSummary } from "../types";

const stageLabels: Record<string, string> = {
  AWAITING_AUDIO: "Aguardando Ã¡udio",
  AWAITING_PROMPTS: "Aguardando prompts",
  READY_FOR_FLOW: "Pronto para o Flow",
  GENERATING_ASSETS: "Gerando assets",
};

function basename(path: string | null) {
  if (!path) return "Ainda nÃ£o gerado";
  return path.split(/[\\/]/).pop() ?? path;
}

function ProjectStrip({
  projects,
  selected,
  onSelect,
}: {
  projects: ProjectSummary[];
  selected: ProjectSummary | null;
  onSelect: (id: string) => void;
}) {
  if (!projects.length) return null;
  return (
    <div className="project-strip">
      <span>PRODUÃ‡ÃƒO</span>
      {projects.map((project) => (
        <button
          className={project.localProjectId === selected?.localProjectId ? "active" : ""}
          key={project.localProjectId}
          onClick={() => onSelect(project.localProjectId)}
        >
          {project.title}
        </button>
      ))}
    </div>
  );
}

function EmptyModule({ title, description, action, onAction }: { title: string; description: string; action: string; onAction: () => void }) {
  return (
    <div className="module-empty">
      <span>â–¶</span>
      <h2>{title}</h2>
      <p>{description}</p>
      <button className="secondary-button" onClick={onAction}>{action}</button>
    </div>
  );
}

export function OverviewView({
  projects,
  bridgeConnected,
  onNavigate,
  onSelect,
  onCreate,
}: {
  projects: ProjectSummary[];
  bridgeConnected: boolean;
  onNavigate: (section: string) => void;
  onSelect: (id: string) => void;
  onCreate: () => void;
}) {
  const totalAssets = projects.reduce((sum, project) => sum + project.assetCount, 0);
  const ready = projects.filter((project) => project.stage === "READY_FOR_FLOW").length;
  return (
    <section className="module-page">
      <div className="module-hero">
        <div>
          <span className="section-kicker">MESA DE PRODUÃ‡ÃƒO</span>
          <h2>Da narraÃ§Ã£o aos assets, sem perder a ordem.</h2>
          <p>Acompanhe o prÃ³ximo passo de cada produÃ§Ã£o e entre direto no ponto que precisa de atenÃ§Ã£o.</p>
        </div>
        <button className="dispatch-button" onClick={onCreate}><span className="mini-play">â–¶</span> Nova produÃ§Ã£o</button>
      </div>
      <div className="metric-ribbon">
        <div><small>PRODUÃ‡Ã•ES</small><strong>{projects.length}</strong><span>pastas locais</span></div>
        <div><small>SLOTS NARRATIVOS</small><strong>{totalAssets}</strong><span>ordem preservada</span></div>
        <div><small>PRONTAS PARA FLOW</small><strong>{ready}</strong><span>prompts validados</span></div>
        <div><small>PONTE FLOW</small><strong className={bridgeConnected ? "bridge-on-value" : "hold-value"}>{bridgeConnected ? "ON" : "OFF"}</strong><span>{bridgeConnected ? "extensÃ£o conectada" : "extensÃ£o desconectada"}</span></div>
      </div>
      <div className="overview-grid">
        <section className="module-card span-two">
          <div className="module-card-head"><div><span className="section-kicker">PRÃ“XIMAS AÃ‡Ã•ES</span><h3>Fila de trabalho</h3></div></div>
          <div className="action-list">
            {projects.map((project) => (
              <button key={project.localProjectId} onClick={() => {
                onSelect(project.localProjectId);
                onNavigate(project.stage === "AWAITING_AUDIO" ? "sincronizacao" : "producoes");
              }}>
                <span className="action-index">â–¶</span>
                <span><strong>{project.title}</strong><small>{stageLabels[project.stage]}</small></span>
                <em>{project.assetCount || "â€”"} slots</em>
              </button>
            ))}
            {!projects.length && <p className="inline-empty">Crie uma produÃ§Ã£o para iniciar a fila de trabalho.</p>}
          </div>
        </section>
        <section className="module-card">
          <div className="module-card-head"><div><span className="section-kicker">GUARDRAILS</span><h3>OperaÃ§Ã£o protegida</h3></div><span className="panel-state done">ATIVO</span></div>
          <div className="status-stack">
            <span><i className="ok" /><strong>Token local</strong><small>SessÃ£o autenticada</small></span>
            <span><i className="ok" /><strong>Fila configurÃ¡vel</strong><small>ConcorrÃªncia definida por execuÃ§Ã£o</small></span>
            <span><i className={bridgeConnected ? "ok" : ""} /><strong>ExtensÃ£o Flow</strong><small>{bridgeConnected ? "Heartbeat local ativo" : "Ainda nÃ£o conectada"}</small></span>
          </div>
        </section>
      </div>
    </section>
  );
}

export function SynchronizationView({
  projects,
  selected,
  onSelect,
  busy,
  pendingAudioPath,
  assetSrtMode,
  onAssetSrtModeChange,
  assetSrtValue,
  onAssetSrtValueChange,
  transitionMode,
  onTransitionModeChange,
  onChooseAudio,
  onProcessAudio,
  onDownload,
  onNavigate,
}: {
  projects: ProjectSummary[];
  selected: ProjectSummary | null;
  onSelect: (id: string) => void;
  busy: boolean;
  pendingAudioPath: string | null;
  assetSrtMode: string;
  onAssetSrtModeChange: (value: string) => void;
  assetSrtValue: number;
  onAssetSrtValueChange: (value: number) => void;
  transitionMode: string;
  onTransitionModeChange: (value: string) => void;
  onChooseAudio: () => void;
  onProcessAudio: () => void;
  onDownload: (kind: "captions" | "assets") => void;
  onNavigate: (section: string) => void;
}) {
  const hasGeneratedSrts = Boolean(selected?.captionSrtPath || selected?.assetSrtPath);
  const currentAudioPath = selected?.audioPath ?? pendingAudioPath ?? null;

  return (
    <section className="module-page simple-page">
      <ProjectStrip projects={projects} selected={selected} onSelect={onSelect} />
      {!selected ? (
        <EmptyModule title="Crie uma produÃ§Ã£o primeiro" description="DÃª um nome ao trabalho e depois envie o Ã¡udio." action="Criar produÃ§Ã£o" onAction={() => onNavigate("producoes")} />
      ) : (
        <>
          <div className="simple-step">
            <div className="step-number">01</div>
            <div className="step-copy">
              <span className="section-kicker">SINCRONIZAÃ‡ÃƒO SRT</span>
              <h2>{hasGeneratedSrts ? "SRTs prontos para baixar" : pendingAudioPath ? "Ãudio pronto para processar" : "Envie a narraÃ§Ã£o"}</h2>
              <p>{hasGeneratedSrts ? "Baixe os dois arquivos. Use o SRT de assets para criar os prompts em sua ferramenta preferida." : pendingAudioPath ? "O Ã¡udio foi selecionado. Revise o arquivo e depois gere os dois SRTs." : "Selecione o Ã¡udio da narraÃ§Ã£o. Os arquivos SRT e o Ã¡udio copiado ficam salvos dentro da pasta local da produÃ§Ã£o."}</p>
              <p>Escolha um Ãºnico critÃ©rio para montar o SRT de assets: palavras, segundos ou pausa. No modo por pausa, o sistema ainda respeita o teto tÃ©cnico de 8s por asset quando necessÃ¡rio.</p>
              <p><strong>Ãudio salvo no projeto:</strong> {basename(currentAudioPath)}</p>
            </div>
            <div className="session-actions" style={{ alignItems: "stretch" }}>
              <div style={{ display: "grid", gap: "10px", minWidth: "280px" }}>
                <label>
                  <span>CritÃ©rio do SRT de assets</span>
                  <select value={assetSrtMode} onChange={(event) => onAssetSrtModeChange(event.target.value)}>
                    <option value="words">Por palavras</option>
                    <option value="seconds">Por segundos</option>
                    <option value="pause">Por pausa</option>
                  </select>
                </label>
                <label>
                  <span>
                    {assetSrtMode === "words"
                      ? "Quantidade de palavras"
                      : assetSrtMode === "seconds"
                        ? "Quantidade de segundos"
                        : "Quantidade de milissegundos"}
                  </span>
                  <input
                    type="number"
                    min={1}
                    max={assetSrtMode === "seconds" ? 8 : assetSrtMode === "pause" ? 10000 : 100}
                    value={assetSrtValue}
                    onChange={(event) => onAssetSrtValueChange(Number(event.target.value) || 1)}
                  />
                </label>
                {assetSrtMode === "pause" && (
                  <label>
                    <span>Troca visual durante a pausa</span>
                    <select value={transitionMode} onChange={(event) => onTransitionModeChange(event.target.value)}>
                      <option value="midpoint">No meio da pausa</option>
                      <option value="next-speech">No inÃ­cio da prÃ³xima fala</option>
                      <option value="previous-speech">No fim da fala anterior</option>
                    </select>
                  </label>
                )}
              </div>
              <div className="session-actions">
                <button className="secondary-button" disabled={busy} onClick={onChooseAudio}>
                  {currentAudioPath ? "Trocar Ã¡udio" : "Selecionar Ã¡udio"}
                </button>
                <button className="dispatch-button" disabled={busy || !currentAudioPath} onClick={onProcessAudio}>
                  <span className="mini-play">â–¶</span>{busy ? "Gerando SRTs..." : "Gerar SRT"}
                </button>
              </div>
            </div>
          </div>
          {hasGeneratedSrts && (
            <div className="download-list">
              <article>
                <span className="file-mark">CC</span>
                <div><strong>SRT de legendas</strong><small>{basename(selected.captionSrtPath)}</small><p>Para adicionar legendas ao vÃ­deo final.</p></div>
                <button className="secondary-button" onClick={() => onDownload("captions")}>Baixar SRT</button>
              </article>
              <article>
                <span className="file-mark asset">A8</span>
                <div><strong>SRT de assets</strong><small>{basename(selected.assetSrtPath)}</small><p>{selected.assetCount} intervalos para gerar os prompts visuais com o critÃ©rio selecionado.</p></div>
                <button className="secondary-button" onClick={() => onDownload("assets")}>Baixar SRT</button>
              </article>
              <button className="quiet-button next-step-button" onClick={() => onNavigate("producoes")}>JÃ¡ tenho os prompts, continuar para ProduÃ§Ãµes â†’</button>
            </div>
          )}
        </>
      )}
    </section>
  );
}

export function SessionsView({
  bridge,
  assembly,
  busy,
  assemblyBusy,
  onOpenBrowser,
  onRefresh,
  onSaveAssemblyKeys,
  onClearAssemblyKeys,
}: {
  bridge: FlowBridgeStatus;
  assembly: AssemblyAiStatus;
  busy: boolean;
  assemblyBusy: boolean;
  onOpenBrowser: () => void;
  onRefresh: () => void;
  onSaveAssemblyKeys: (keys: string) => Promise<boolean>;
  onClearAssemblyKeys: () => void;
}) {
  const [assemblyKeys, setAssemblyKeys] = useState("");
  const saveAssemblyKeys = async () => {
    if (await onSaveAssemblyKeys(assemblyKeys)) setAssemblyKeys("");
  };
  return (
    <section className="module-page simple-page settings-page">
      <div className="step-copy">
        <span className="section-kicker">CONFIGURAÃ‡Ã•ES</span>
        <h2>IntegraÃ§Ãµes</h2>
        <p>Essas configuraÃ§Ãµes sÃ£o feitas uma vez. Durante o trabalho, o navegador Flow permanece minimizado.</p>
      </div>
      <section className="module-card simple-setting-card">
        <div>
          <span className={`pulse-dot ${bridge.extensionConnected ? "" : "offline"}`} />
          <span><strong>Ponte com Google Flow</strong><small>{bridge.extensionConnected ? "Conectada e pronta para receber comandos" : bridge.extensionInstalled ? "ExtensÃ£o instalada; reconecte a sessÃ£o" : "InstalaÃ§Ã£o inicial necessÃ¡ria"}</small></span>
        </div>
        <div className="session-actions">
          <button className="quiet-button" onClick={onRefresh}>Atualizar</button>
          <button className="secondary-button" disabled={busy} onClick={onOpenBrowser}>{busy ? "Abrindo..." : bridge.extensionConnected ? "Reabrir sessÃ£o" : "Conectar Flow"}</button>
        </div>
      </section>
      <section className="module-card integration-card">
        <div className="module-card-head">
          <div><span className="section-kicker">TRANSCRICAO</span><h3>AssemblyAI</h3></div>
          <span className={`panel-state ${assembly.configured ? "done" : ""}`}>{assembly.configured ? "CONFIGURADA" : "CONFIGURAR"}</span>
        </div>
        <div className="integration-layout">
          <div className="integration-status">
            <span><small>CHAVES DISPONIVEIS</small><strong>{assembly.keyCount}</strong></span>
            <p>{assembly.configured ? `Configuradas: ${assembly.maskedKeys.join(", ")}` : "Adicione uma chave para liberar o processamento de audio."}</p>
            <p className="bridge-privacy-note">As chaves ficam no diretorio local do aplicativo, nao retornam para a interface e sao mascaradas nos diagnosticos.</p>
          </div>
          <div className="integration-form">
            <label>
              <span>Chave da AssemblyAI</span>
              <textarea
                rows={3}
                value={assemblyKeys}
                onChange={(event) => setAssemblyKeys(event.target.value)}
                placeholder="Cole uma chave por linha"
              />
            </label>
            <div className="integration-actions">
              {assembly.configured && <button className="quiet-button" disabled={assemblyBusy} onClick={onClearAssemblyKeys}>Remover chaves</button>}
              <button className="secondary-button" disabled={!assemblyKeys.trim() || assemblyBusy} onClick={saveAssemblyKeys}>
                {assemblyBusy ? "Salvando..." : "Salvar configuracao"}
              </button>
            </div>
          </div>
        </div>
      </section>
    </section>
  );
}
