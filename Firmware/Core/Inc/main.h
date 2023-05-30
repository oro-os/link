/* USER CODE BEGIN Header */
/**
  ******************************************************************************
  * @file           : main.h
  * @brief          : Header for main.c file.
  *                   This file contains the common defines of the application.
  ******************************************************************************
  * @attention
  *
  * Copyright (c) 2023 STMicroelectronics.
  * All rights reserved.
  *
  * This software is licensed under terms that can be found in the LICENSE file
  * in the root directory of this software component.
  * If no LICENSE file comes with this software, it is provided AS-IS.
  *
  ******************************************************************************
  */
/* USER CODE END Header */

/* Define to prevent recursive inclusion -------------------------------------*/
#ifndef __MAIN_H
#define __MAIN_H

#ifdef __cplusplus
extern "C" {
#endif

/* Includes ------------------------------------------------------------------*/
#include "stm32f4xx_hal.h"

/* Private includes ----------------------------------------------------------*/
/* USER CODE BEGIN Includes */

/* USER CODE END Includes */

/* Exported types ------------------------------------------------------------*/
/* USER CODE BEGIN ET */

/* USER CODE END ET */

/* Exported constants --------------------------------------------------------*/
/* USER CODE BEGIN EC */

/* USER CODE END EC */

/* Exported macro ------------------------------------------------------------*/
/* USER CODE BEGIN EM */

/* USER CODE END EM */

/* Exported functions prototypes ---------------------------------------------*/
void Error_Handler(void);

/* USER CODE BEGIN EFP */

/* USER CODE END EFP */

/* Private defines -----------------------------------------------------------*/
#define OLED_RST_Pin GPIO_PIN_13
#define OLED_RST_GPIO_Port GPIOC
#define OLED_DC_Pin GPIO_PIN_14
#define OLED_DC_GPIO_Port GPIOC
#define CLOCK_IN_Pin GPIO_PIN_0
#define CLOCK_IN_GPIO_Port GPIOH
#define OLED_MOSI_Pin GPIO_PIN_3
#define OLED_MOSI_GPIO_Port GPIOC
#define SYSETH_XFRM_EN_Pin GPIO_PIN_3
#define SYSETH_XFRM_EN_GPIO_Port GPIOA
#define SYSETH_SS_Pin GPIO_PIN_4
#define SYSETH_SS_GPIO_Port GPIOA
#define SYSETH_SCK_Pin GPIO_PIN_5
#define SYSETH_SCK_GPIO_Port GPIOA
#define SYSETH_MISO_Pin GPIO_PIN_6
#define SYSETH_MISO_GPIO_Port GPIOA
#define SYSETH_MOSI_Pin GPIO_PIN_7
#define SYSETH_MOSI_GPIO_Port GPIOA
#define SYSETH_INT_Pin GPIO_PIN_0
#define SYSETH_INT_GPIO_Port GPIOB
#define SYSETH_INT_EXTI_IRQn EXTI0_IRQn
#define SYSETH_RST_Pin GPIO_PIN_1
#define SYSETH_RST_GPIO_Port GPIOB
#define UART_RX_Pin GPIO_PIN_7
#define UART_RX_GPIO_Port GPIOE
#define UART_TX_Pin GPIO_PIN_8
#define UART_TX_GPIO_Port GPIOE
#define DBGLED_Pin GPIO_PIN_12
#define DBGLED_GPIO_Port GPIOE
#define RS232_TX_Pin GPIO_PIN_10
#define RS232_TX_GPIO_Port GPIOB
#define RS232_RX_Pin GPIO_PIN_11
#define RS232_RX_GPIO_Port GPIOB
#define RS232_CTS_Pin GPIO_PIN_13
#define RS232_CTS_GPIO_Port GPIOB
#define RS232_RTS_Pin GPIO_PIN_14
#define RS232_RTS_GPIO_Port GPIOB
#define SYS_POWER_Pin GPIO_PIN_8
#define SYS_POWER_GPIO_Port GPIOC
#define SYS_RESET_Pin GPIO_PIN_9
#define SYS_RESET_GPIO_Port GPIOC
#define USB_D__Pin GPIO_PIN_11
#define USB_D__GPIO_Port GPIOA
#define USB_D_A12_Pin GPIO_PIN_12
#define USB_D_A12_GPIO_Port GPIOA
#define SWDIO_Pin GPIO_PIN_13
#define SWDIO_GPIO_Port GPIOA
#define SWCLK_Pin GPIO_PIN_14
#define SWCLK_GPIO_Port GPIOA
#define EXTETH_SS_Pin GPIO_PIN_15
#define EXTETH_SS_GPIO_Port GPIOA
#define EXTETH_SCK_Pin GPIO_PIN_10
#define EXTETH_SCK_GPIO_Port GPIOC
#define EXTETH_MISO_Pin GPIO_PIN_11
#define EXTETH_MISO_GPIO_Port GPIOC
#define EXTETH_MOSI_Pin GPIO_PIN_12
#define EXTETH_MOSI_GPIO_Port GPIOC
#define EXTETH_RST_Pin GPIO_PIN_0
#define EXTETH_RST_GPIO_Port GPIOD
#define EXTETH_INT_Pin GPIO_PIN_1
#define EXTETH_INT_GPIO_Port GPIOD
#define EXTETH_INT_EXTI_IRQn EXTI1_IRQn
#define EXTETH_XFRM_EN_Pin GPIO_PIN_2
#define EXTETH_XFRM_EN_GPIO_Port GPIOD
#define OLED_SCK_Pin GPIO_PIN_3
#define OLED_SCK_GPIO_Port GPIOD
#define PSU_OK_Pin GPIO_PIN_5
#define PSU_OK_GPIO_Port GPIOD
#define PSU_ON_Pin GPIO_PIN_6
#define PSU_ON_GPIO_Port GPIOD
#define SWO_Pin GPIO_PIN_3
#define SWO_GPIO_Port GPIOB
#define INDLIGHTS_SCL_Pin GPIO_PIN_6
#define INDLIGHTS_SCL_GPIO_Port GPIOB
#define INDLIGHTS_SDA_Pin GPIO_PIN_7
#define INDLIGHTS_SDA_GPIO_Port GPIOB
#define OLED_SS_Pin GPIO_PIN_9
#define OLED_SS_GPIO_Port GPIOB

/* USER CODE BEGIN Private defines */

/* USER CODE END Private defines */

#ifdef __cplusplus
}
#endif

#endif /* __MAIN_H */
