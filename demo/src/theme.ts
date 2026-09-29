import type {GlobalThemeOverrides} from 'naive-ui'

export const workbenchTheme: GlobalThemeOverrides = {
    common: {
        primaryColor: '#0878ed',
        primaryColorHover: '#2589f4',
        primaryColorPressed: '#0065cc',
        primaryColorSuppl: '#0878ed',
        successColor: '#248a3d',
        warningColor: '#9a6700',
        errorColor: '#d93436',
        textColorBase: '#233044',
        textColor1: '#233044',
        textColor2: '#4f5e72',
        textColor3: '#6b778b',
        borderColor: 'rgba(103, 125, 153, 0.22)',
        borderRadius: '8px',
        fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif',
        fontSize: '13px',
        heightMedium: '36px',
        heightLarge: '40px',
        boxShadow2: '0 14px 44px rgba(45, 66, 99, 0.16)',
    },
    Button: {fontWeight: '500', borderRadiusMedium: '9px', borderRadiusLarge: '10px', borderRadiusSmall: '7px'},
    Input: {
        color: 'rgba(255, 255, 255, 0.54)',
        colorFocus: 'rgba(255, 255, 255, 0.85)',
        colorDisabled: 'rgba(233, 239, 247, 0.5)',
        boxShadowFocus: '0 0 0 3px rgba(8, 120, 237, 0.15)'
    },
    InternalSelection: {
        color: 'rgba(255, 255, 255, 0.54)',
        colorActive: 'rgba(255, 255, 255, 0.85)',
        colorDisabled: 'rgba(233, 239, 247, 0.5)',
        boxShadowFocus: '0 0 0 3px rgba(8, 120, 237, 0.15)'
    },
    Form: {
        labelTextColor: '#546276',
        labelFontSizeTopMedium: '12px',
        labelFontWeight: '500',
        labelHeightMedium: '22px',
        feedbackHeightMedium: '12px'
    },
    Tabs: {
        tabBorderRadius: '7px',
        colorSegment: 'rgba(119, 144, 179, 0.1)',
        tabColorSegment: 'rgba(255, 255, 255, 0.9)',
        tabTextColorActiveSegment: '#233044',
        tabFontWeightActive: '500'
    },
    Alert: {borderRadius: '10px'},
    DataTable: {
        thColor: 'rgba(224, 233, 246, 0.5)',
        thTextColor: '#66758a',
        tdColor: 'rgba(255, 255, 255, 0.3)',
        tdColorHover: 'rgba(255, 255, 255, 0.65)',
        borderColor: 'rgba(107, 130, 162, 0.15)',
        borderRadius: '10px'
    },
}
