"""Offline screenshot + smoke suite for ALIA design prototype.

Requirements: Python + playwright + Chromium. The suite embeds local CSS/JS into
an about:blank document to avoid network and remains independent of any server.
"""
from pathlib import Path
import asyncio, json, datetime
from playwright.async_api import async_playwright

ROOT = Path(__file__).resolve().parents[1]
PROTO = ROOT / 'prototype'
SCREENS = PROTO / 'screens'
SCREENS.mkdir(parents=True, exist_ok=True)


def inline_document():
    html = (PROTO / 'index.html').read_text(encoding='utf-8')
    for name in ('base', 'theme'):
        tag = f'<link rel="stylesheet" href="./assets/{name}.css" />'
        html = html.replace(tag, '<style>' + (PROTO / f'assets/{name}.css').read_text(encoding='utf-8') + '</style>')
    html = html.replace('<script src="./assets/app.js" defer></script>', '<script>' + (PROTO / 'assets/app.js').read_text(encoding='utf-8') + '</script>')
    return html


async def main():
    results = []
    errors = []
    doc = inline_document()

    def done(name, passed, detail=''):
        results.append({'test': name, 'status': 'PASS' if passed else 'FAIL', 'detail': str(detail)})
        print(('PASS ' if passed else 'FAIL ') + name + ' ' + str(detail))

    async with async_playwright() as p:
        browser = await p.chromium.launch(executable_path='/usr/bin/chromium', args=['--no-sandbox', '--disable-dev-shm-usage'])
        page = await browser.new_page(viewport={'width': 1440, 'height': 1080}, device_scale_factor=1)
        page.on('pageerror', lambda err: errors.append(str(err)))
        await page.set_content(doc, wait_until='load')
        done('startup-no-js-errors', len(errors) == 0, errors)
        done('default-light-theme', await page.evaluate("document.documentElement.classList.contains('light')"))
        done('no-character-images', (await page.locator('img,video,canvas').count()) == 0)
        done('has-10-navigation-routes', len(set(await page.locator('.nav-button[data-nav]').evaluate_all('(els)=>els.map(x=>x.dataset.nav)'))) == 10)
        await page.screenshot(path=str(SCREENS / 'overview-desktop.png'), full_page=True)

        for route, heading in [
            ('overview','数字个体控制台'),
            ('identity','身份锚点与人格边界'),
            ('memory','记忆不是一份可以随意重写的简介'),
            ('cognition','认知'),
            ('relations','关系不是一项亲密度分数'),
            ('agency','行动'),
            ('media','媒介'),
            ('governance','授权'),
            ('events','行为与状态审计记录'),
            ('settings','运行状态')
        ]:
            await page.locator(f'.nav-button[data-nav="{route}"]').click()
            body=await page.locator('#content').inner_text()
            done('page-'+route, heading in body, 'matched text '+heading)
            if route == 'memory':
                await page.screenshot(path=str(SCREENS / 'memory-desktop.png'), full_page=True)
            if route == 'governance':
                await page.screenshot(path=str(SCREENS / 'governance-desktop.png'), full_page=True)
            if route == 'agency':
                await page.screenshot(path=str(SCREENS / 'agency-desktop.png'), full_page=True)
        await page.locator('.nav-button[data-nav="memory"]').click()
        n_before=await page.locator('.memory-item').count()
        await page.locator('#memorySearch').fill('确实无关的超级罕见词XYZ')
        n_after=await page.locator('.memory-item').count()
        done('memory-search-empty', n_before>0 and n_after==0 and '没有匹配' in await page.locator('#content').inner_text(), f'{n_before}->{n_after}')
        await page.locator('#memorySearch').fill('身份')
        done('memory-search-restores', await page.locator('.memory-item').count()>=1)
        await page.locator('.nav-button[data-nav="overview"]').click()
        await page.locator('[data-action="approve:a01"]').click()
        has_modal = await page.locator('#modalRoot').inner_text()
        done('approval-shows-confirmation', '确认' in has_modal or '批准' in has_modal, has_modal[:80])
        await page.locator('[data-action="confirm-modal"]').click()
        done('approval-mock-updates', '1 项待审' in await page.locator('#content').inner_text())
        await page.locator('[data-action="theme"]').click()
        done('theme-switch', not await page.evaluate("document.documentElement.classList.contains('light')"))

        for width,name in [(1440,'wide'),(390,'mobile'),(360,'small-mobile')]:
            await page.set_viewport_size({'width':width,'height':844 if width<500 else 1080})
            overflow=await page.evaluate('document.documentElement.scrollWidth-document.documentElement.clientWidth')
            done('no-horizontal-overflow-'+name,overflow<=1,f'excess px={overflow}')
            if width==390:
                await page.locator('[data-action="theme"]').click()
                await page.wait_for_timeout(380)
                await page.locator('[data-action="menu"]').click()
                done('mobile-drawer-open', await page.locator('#app').evaluate("el=>el.classList.contains('menu-open')"))
                await page.locator('.nav-button[data-nav="memory"]').click()
                done('mobile-drawer-nav-closes', not await page.locator('#app').evaluate("el=>el.classList.contains('menu-open')"))


        done('end-no-js-errors',len(errors)==0,errors)
        # Clean mobile screenshots (no operation toast, no partially animated drawer).
        fresh = await browser.new_page(viewport={'width':390,'height':844},device_scale_factor=1)
        fresh.on('pageerror', lambda err: errors.append(str(err)))
        await fresh.set_content(doc,wait_until='load')
        await fresh.wait_for_timeout(400)
        await fresh.screenshot(path=str(SCREENS / 'overview-mobile.png'),full_page=True)
        await fresh.locator('[data-action="menu"]').click()
        await fresh.wait_for_timeout(260)
        await fresh.locator('.nav-button[data-nav="memory"]').click()
        await fresh.wait_for_timeout(280)
        await fresh.screenshot(path=str(SCREENS / 'memory-mobile.png'),full_page=True)
        await browser.close()

    report={'product':'ALIA UIUX no character v2.0','date':'2026-11-11', 'target':'interactive prototype, not production Agent','total':len(results),'passed':sum(r['status']=='PASS' for r in results),'failed':sum(r['status']=='FAIL' for r in results),'checks':results}
    (ROOT/'qa/smoke-test.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print('RESULT',report['passed'],'/',report['total'])
    if report['failed']:
        raise SystemExit(1)

if __name__=='__main__': asyncio.run(main())
