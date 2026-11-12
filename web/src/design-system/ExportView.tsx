import {useState} from 'react';
import {Confirm} from './components';

/** Exports an explicitly selected UI projection, never the controller or API client. */
export function ExportView({name,description,projection}:{name:string;description:string;projection:unknown}) {
  const [snapshot,setSnapshot]=useState<string|null>(null);
  const [notice,setNotice]=useState('');
  async function download() {
    if(snapshot===null)return false;
    const url=URL.createObjectURL(new Blob([snapshot],{type:'application/json;charset=utf-8'}));
    const link=document.createElement('a');
    link.href=url;link.download=`alia-${name}-${new Date().toISOString().replace(/[:.]/g,'-')}.json`;
    document.body.append(link);
    try{link.click();setNotice('已发起当前视图下载；请在浏览器下载列表查看。');}
    finally{link.remove();setTimeout(()=>URL.revokeObjectURL(url),1000);}
    return true;
  }
  return <><button onClick={()=>{
    setNotice('');setSnapshot(JSON.stringify({format:'alia-ui-view/1',exported_at:new Date().toISOString(),scope:name,complete_backup:false,view:projection},null,2));
  }}>导出当前视图</button>{notice&&<p role="status">{notice}</p>}
  {snapshot!==null&&<Confirm title="导出当前视图" busy={false} onClose={()=>setSnapshot(null)} action={download}>
    <p>{description}</p><p>仅下载已加载的页面数据，不是完整审计或可恢复备份。文件可能含私人信息，请妥善保存。此次操作不修改服务端状态。</p>
    <details><summary>检查将要下载的内容</summary><pre>{snapshot}</pre></details>
  </Confirm>}</>;
}
